/*!
# Websockets

Build a WebSocket Client to receive real-time information from Warframe Market

To build a new WsClient, please see the [`Client`][crate::Client] documentation

## Record active users
```rust
use wf_market::{
    error::WsError,
    client::ws::WsClient,
    Client,
};

#[tokio::main]
async fn main() -> Result<(), WsError> {
    let mut client = {
        Client::new()
            .login("user", "pass", "dev").await.unwrap()
    };

    let client = client.create_websocket()
        .register_callback("event/reports/online", |msg, _, _| {
            let payload = msg.payload.clone().unwrap();
            println!("Users Online: {}", payload.get("authorizedUsers").unwrap().as_i64());
            Ok(())
        })?
        .build().await?;

    tokio::signal::ctrl_c().await.unwrap() // Our client is not long-running, so to keep the WsClient in scope we need to never return
}
```
Note:
-- Use internal/connected and internal/disconnected to handle connection state
*/

use crate::error::WsError;
use futures_util::stream::{AbortHandle, Abortable};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

pub(super) const WS_URL: &'static str = "wss://warframe.market/socket-v2";

// Uncomment for local testing
// pub(super) const WS_URL: &'static str = "ws://localhost:7369";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WsMessage {
    pub route: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "refId", skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
}
impl WsMessage {
    pub fn new(route: &str, payload: Option<serde_json::Value>) -> Self {
        WsMessage {
            route: route.to_string(),
            payload,
            id: Some(uuid::Uuid::new_v4().to_string()),
            ref_id: None,
        }
    }
    pub fn connect() -> Self {
        WsMessage {
            route: "@internal|internal/connected".to_string(),
            payload: Some(json!({"status": "connected"})),
            id: Some("INTERNAL".to_string()),
            ref_id: None,
        }
    }
    pub fn disconnect(error: String) -> Self {
        WsMessage {
            route: "@internal|internal/disconnected".to_string(),
            payload: Some(json!({"reason": error})),
            id: Some("INTERNAL".to_string()),
            ref_id: None,
        }
    }
}

// Route structure with parameter support
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    pub protocol: String,
    pub path: String,
    pub parameter: Option<String>,
}

impl Route {
    pub fn parse(route_str: &str) -> Result<Self, WsError> {
        if let Some(pipe_pos) = route_str.find('|') {
            let protocol = route_str[..pipe_pos].to_string();
            let path_and_param = &route_str[pipe_pos + 1..];

            // Check for parameter (after colon)
            if let Some(colon_pos) = path_and_param.find(':') {
                let path = path_and_param[..colon_pos].to_string();
                let parameter = Some(path_and_param[colon_pos + 1..].to_string());
                Ok(Route {
                    protocol,
                    path,
                    parameter,
                })
            } else {
                let path = path_and_param.to_string();
                Ok(Route {
                    protocol,
                    path,
                    parameter: None,
                })
            }
        } else {
            Err(WsError::InvalidPath(route_str.to_string()))
        }
    }

    pub fn to_string(&self) -> String {
        match &self.parameter {
            Some(param) => format!("{}|{}:{}", self.protocol, self.path, param),
            None => format!("{}|{}", self.protocol, self.path),
        }
    }

    // Get the base path without parameter for routing
    pub fn base_path(&self) -> &str {
        &self.path
    }

    // Get the full path with parameter for exact matching
    pub fn full_path(&self) -> String {
        match &self.parameter {
            Some(param) => format!("{}:{}", self.path, param),
            None => self.path.clone(),
        }
    }
}

// Message sender handle that can be cloned and passed to callbacks
#[derive(Clone)]
pub struct MessageSender {
    tx: mpsc::UnboundedSender<WsMessage>,
}

impl MessageSender {
    pub fn send_message(&self, message: WsMessage) -> Result<(), WsError> {
        self.tx
            .send(message)
            .map_err(|e| WsError::SendError(e.to_string()))?;
        Ok(())
    }

    pub fn send_response(
        &self,
        route: &str,
        payload: serde_json::Value,
        ref_id: &str,
    ) -> Result<(), WsError> {
        let message = WsMessage {
            route: route.to_string(),
            payload: Some(payload),
            id: Some(uuid::Uuid::new_v4().to_string()),
            ref_id: Some(ref_id.to_string()),
        };
        self.send_message(message)
    }

    pub fn send_request(&self, route: &str, payload: serde_json::Value) -> Result<String, WsError> {
        let id = uuid::Uuid::new_v4().to_string();
        let message = WsMessage {
            route: route.to_string(),
            payload: Some(payload),
            id: Some(id.clone()),
            ref_id: None,
        };
        self.send_message(message)?;
        Ok(id)
    }
}

// Updated callback type to include sender and route info
pub type MessageCallback =
    Arc<dyn Fn(&WsMessage, &Route, &MessageSender) -> Result<(), WsError> + Send + Sync>;

// Internal router
pub(crate) struct Router {
    routes: HashMap<String, MessageCallback>,
}

impl Router {
    fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    // Internal reserved paths that the client uses
    fn get_reserved_paths() -> Vec<&'static str> {
        vec!["cmd/auth/signIn"]
    }

    fn is_path_reserved(path: &str) -> bool {
        Self::get_reserved_paths().contains(&path)
    }

    fn register(&mut self, path: &str, callback: MessageCallback) -> Result<(), WsError> {
        // Check if path is reserved by the client
        if Self::is_path_reserved(path) {
            return Err(WsError::ReservedPath(path.to_string()));
        }

        // Check if already registered
        if self.routes.contains_key(path) {
            return Err(WsError::AlreadyRegistered(path.to_string()));
        }

        self.routes.insert(path.to_string(), callback);
        Ok(())
    }

    fn route_message(&self, message: &WsMessage, sender: &MessageSender) -> Result<(), WsError> {
        let route = Route::parse(&message.route)?;

        // Handle internal routes first
        if Self::is_path_reserved(route.base_path()) {
            self.handle_internal_route(&route, message, sender)?;
            return Ok(());
        }

        // Try to find callback with routing priority:
        // 1. Exact match with parameter (e.g., "cmd/subscribe/newOrders:ok")
        // 2. Base path match (e.g., "cmd/subscribe/newOrders")

        let callback = self
            .routes
            .get(&route.full_path())
            .or_else(|| self.routes.get(route.base_path()));

        if let Some(callback) = callback {
            callback(message, &route, sender)?;
        } else {
            // Optionally log unhandled routes
            println!(
                "No handler for route: {} (full: {})",
                route.base_path(),
                route.full_path()
            );
        }

        Ok(())
    }

    // Handle internal client routes
    fn handle_internal_route(
        &self,
        route: &Route,
        _message: &WsMessage,
        sender: &MessageSender,
    ) -> Result<(), WsError> {
        match route.base_path() {
            "cmd/auth/signIn" => {
                println!(
                    "Handling internal auth sign in with parameter: {:?}",
                    route.parameter
                );
                // Example: Handle different auth responses based on parameter
                match route.parameter.as_deref() {
                    Some("ok") => {
                        if let Some(connected_callback) = self.routes.get("internal/auth_connected")
                        {
                            let route = Route {
                                protocol: "@internal".to_string(),
                                path: "internal/auth_connected".to_string(),
                                parameter: None,
                            };
                            connected_callback(
                                &WsMessage {
                                    route: "@internal|internal/auth_connected".to_string(),
                                    payload: Some(serde_json::Value::from(true)),
                                    id: Some("INTERNAL".to_string()),
                                    ref_id: None,
                                },
                                &route,
                                &sender,
                            )?;
                        }
                    }
                    Some("error") => println!("Auth failed"),
                    _ => println!("Unknown auth response"),
                }
            }
            _ => {
                println!(
                    "Unhandled internal route: {} (parameter: {:?})",
                    route.base_path(),
                    route.parameter
                );
            }
        }
        Ok(())
    }
}

// WebSocket client builder
pub struct WsClientBuilder {
    router: Router,
    token: String,
    device_id: String,
}

impl WsClientBuilder {
    pub(crate) fn new(token: String, device_id: String) -> Self {
        Self {
            router: Router::new(),
            token,
            device_id,
        }
    }

    /// Register a callback for a specific path with optional parameter
    ///
    /// Examples:
    /// - `register_callback("cmd/subscribe/newOrders", callback)` - matches any parameter
    /// - `register_callback("cmd/subscribe/newOrders:ok", callback)` - matches only :ok parameter
    pub fn register_callback<F>(mut self, path: &str, callback: F) -> Result<Self, WsError>
    where
        F: Fn(&WsMessage, &Route, &MessageSender) -> Result<(), WsError> + Send + Sync + 'static,
    {
        self.router.register(path, Arc::new(callback))?;
        Ok(self)
    }

    /// Get list of paths reserved by the client for internal usage
    pub fn get_reserved_paths() -> Vec<&'static str> {
        Router::get_reserved_paths()
    }

    /// Build and start the WebSocket client
    pub async fn build(self) -> Result<WsClient, WsError> {
        let router = Arc::new(self.router);
        let sender_holder = Arc::new(Mutex::new(None));

        tokio::spawn({
            let sender_holder = Arc::clone(&sender_holder);
            let router = Arc::clone(&router);

            async move {
                loop {
                    let mut request = WS_URL.into_client_request().unwrap();
                    let headers = request.headers_mut();
                    headers.append("Sec-WebSocket-Protocol", "wfm".parse().unwrap());
                    headers.append("User-Agent", "wf-market-rs".parse().unwrap());

                    // println!("Attempting to connect to WebSocket...");

                    match connect_async(request).await {
                        Ok((ws_stream, _)) => {
                            // println!("Connected to WebSocket.");
                            let ws_error = Arc::new(Mutex::new(None));
                            let ws_error_write = Arc::clone(&ws_error);
                            let ws_error_read = Arc::clone(&ws_error);
                            let (mut write, read) = ws_stream.split();
                            let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
                            let sender = MessageSender { tx: tx.clone() };

                            // Send connection message to the router
                            WsClient::send_connect_message(&router, &sender).unwrap();

                            // Send authentication
                            let auth_payload = json!({
                                "token": self.token,
                                "deviceId": self.device_id,
                            });
                            match sender.send_request("@wfm|cmd/auth/signIn", auth_payload) {
                                Ok(_) => {}
                                Err(e) => {
                                    eprintln!("Failed to send authentication request: {:?}", e);
                                    continue; // Retry connection
                                }
                            }

                            *sender_holder.lock().unwrap() = Some(sender.clone());

                            // Create an abort handle to control the write task
                            let (abort_handle, abort_registration) = AbortHandle::new_pair();

                            // Write task (wrapped in Abortable) Is responsible for sending messages
                            // It will be aborted if the read task fails or ends
                            let write_task = tokio::spawn(Abortable::new(
                                async move {
                                    let ws_error_write = Arc::clone(&ws_error_write);
                                    while let Some(msg) = rx.recv().await {
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            if let Err(e) = write
                                                .send(Message::Text(Utf8Bytes::from(json)))
                                                .await
                                            {
                                                eprintln!("Write failed: {}", e);
                                                *ws_error_write.lock().unwrap() = Some(e);
                                                break;
                                            }
                                        }
                                    }
                                },
                                abort_registration,
                            ));

                            // Read task (will trigger abort on write if it fails or ends)
                            let read_task = tokio::spawn({
                                let sender = sender.clone();
                                let router = Arc::clone(&router);
                                let abort_handle = abort_handle.clone(); // Move handle in
                                let mut read = read;

                                async move {
                                    let ws_error_read = Arc::clone(&ws_error_read);
                                    while let Some(msg) = read.next().await {
                                        match msg {
                                            Ok(Message::Text(text)) => {
                                                if let Err(e) = WsClient::handle_text_message(
                                                    &router, &text, &sender,
                                                ) {
                                                    eprintln!("Handle error: {:?}", e);
                                                }
                                            }
                                            Ok(Message::Close(_)) => {
                                                println!("Connection closed by server.");
                                                break;
                                            }
                                            Ok(_) => (),
                                            Err(e) => {
                                                eprintln!("Read error: {}", e);
                                                *ws_error_read.lock().unwrap() = Some(e);
                                                break;
                                            }
                                        }
                                    }

                                    // If we exit the read loop, abort the write task
                                    abort_handle.abort();
                                }
                            });

                            // Wait for both tasks
                            let _ = tokio::join!(read_task, write_task);
                            // Send a message to the sender to indicate disconnection
                            WsClient::send_disconnect_message(
                                &router,
                                &WsMessage::disconnect(format!(
                                    "Connection lost: {:?} will retry in 5 seconds",
                                    ws_error.lock().unwrap()
                                )),
                                &sender,
                            )
                            .unwrap();
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }

                        Err(err) => {
                            eprintln!("WebSocket connection failed: {}", err);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        });

        tokio::time::sleep(Duration::from_secs(1)).await;

        Ok(WsClient {
            sender: Arc::clone(&sender_holder),
        })
    }
}

// The actual WebSocket client (runtime instance)
pub struct WsClient {
    sender: Arc<Mutex<Option<MessageSender>>>,
}

impl WsClient {
    pub(crate) fn send_disconnect_message(
        router: &Router,
        message: &WsMessage,
        sender: &MessageSender,
    ) -> Result<(), WsError> {
        router.route_message(&message, sender)
    }
    pub(crate) fn send_connect_message(
        router: &Router,
        sender: &MessageSender,
    ) -> Result<(), WsError> {
        let message = WsMessage::connect();
        router.route_message(&message, sender)
    }
    pub(crate) fn handle_text_message(
        router: &Router,
        text: &str,
        sender: &MessageSender,
    ) -> Result<(), WsError> {
        let message: WsMessage = serde_json::from_str(text)
            .map_err(|_| WsError::InvalidMessageReceived(text.to_string()))?;
        router.route_message(&message, sender)
    }

    // Public methods for sending messages (only available after build)
    pub fn send_message(&self, message: WsMessage) -> Result<(), WsError> {
        let sender_guard = self.sender.lock().unwrap();
        if let Some(sender) = sender_guard.as_ref() {
            sender.send_message(message)
        } else {
            Err(WsError::ConnectionError)
        }
    }

    pub fn send_response(
        &self,
        route: &str,
        payload: serde_json::Value,
        ref_id: &str,
    ) -> Result<(), WsError> {
        let sender_guard = self.sender.lock().unwrap();
        if let Some(sender) = sender_guard.as_ref() {
            sender.send_response(route, payload, ref_id)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn send_request(&self, route: &str, payload: serde_json::Value) -> Result<String, WsError> {
        let route_parsed =
            Route::parse(route).map_err(|_| WsError::InvalidPath(route.to_string()))?;
        if route_parsed.protocol == "internal" {
            return Err(WsError::ReservedPath(
                "Can't send on internal routes".to_string(),
            ));
        }
        let sender_guard = self.sender.lock().unwrap();
        if let Some(sender) = sender_guard.as_ref() {
            sender.send_request(route, payload)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn get_sender(&self) -> Option<MessageSender> {
        self.sender.lock().unwrap().clone()
    }
}
