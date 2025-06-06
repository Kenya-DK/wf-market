/*!
# Websockets

Build a WebSocket Client to receive real-time information from Warframe Market

## Record active users
```rust
use wf_market::{
    error::WsError,
    client::ws::WsClient
};

struct GlobalInfo {
    active_users: u32,
}

#[tokio::main]
async fn main() -> Result<(), WsError> {
    let client = WsClient::new()
        .register_callback("internal/connected", |msg, _, _| {
            println!("WebSocket has connected to the WFM API");
            Ok(())
        })?
        .register_callback("event/reports/online", |msg, _, _| {
            println!("Users Online: {}", msg.payload.unwrap().get("authorizedUsers").unwrap().as_i64());
            Ok(())
        })?
        .build().await?;

    loop { } // Our client is not long-running, so to keep the WsClient in scope we need to never return
}
```
*/

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use tokio_tungstenite::{connect_async};
use tokio_tungstenite::tungstenite::{Error, Message, Utf8Bytes};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use crate::error::WsError;

pub(super) const WS_URL: &'static str = "wss://warframe.market/socket-v2";

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
                Ok(Route { protocol, path, parameter })
            } else {
                let path = path_and_param.to_string();
                Ok(Route { protocol, path, parameter: None })
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
        self.tx.send(message).map_err(|_| WsError::SendError)?;
        Ok(())
    }

    pub fn send_response(
        &self,
        route: &str,
        payload: serde_json::Value,
        ref_id: &str
    ) -> Result<(), WsError> {
        let message = WsMessage {
            route: route.to_string(),
            payload: Some(payload),
            id: Some(uuid::Uuid::new_v4().to_string()),
            ref_id: Some(ref_id.to_string()),
        };
        self.send_message(message)
    }

    pub fn send_request(
        &self,
        route: &str,
        payload: serde_json::Value
    ) -> Result<String, WsError> {
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
pub type MessageCallback = Arc<dyn Fn(&WsMessage, &Route, &MessageSender) -> Result<(), WsError> + Send + Sync>;

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
        vec![
            "cmd/auth/signIn",
        ]
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

        let callback = self.routes.get(&route.full_path())
            .or_else(|| self.routes.get(route.base_path()));

        if let Some(callback) = callback {
            callback(message, &route, sender)?;
        } else {
            // Optionally log unhandled routes
            println!("No handler for route: {} (full: {})", route.base_path(), route.full_path());
        }

        Ok(())
    }

    // Handle internal client routes
    fn handle_internal_route(&self, route: &Route, message: &WsMessage, sender: &MessageSender) -> Result<(), WsError> {
        match route.base_path() {
            "cmd/auth/signIn" => {
                println!("Handling internal auth sign in with parameter: {:?}", route.parameter);
                // Example: Handle different auth responses based on parameter
                match route.parameter.as_deref() {
                    Some("ok") => println!("Auth successful"),
                    Some("error") => println!("Auth failed"),
                    _ => println!("Unknown auth response"),
                }
            }
            _ => {
                println!("Unhandled internal route: {} (parameter: {:?})", route.base_path(), route.parameter);
            }
        }
        Ok(())
    }
}

// WebSocket client builder
pub struct WsClientBuilder {
    router: Router,
}

impl WsClientBuilder {
    fn new() -> Self {
        Self {
            router: Router::new(),
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
        let mut request = WS_URL.into_client_request().unwrap();
        
        let headers = request.headers_mut();
        headers.append("Sec-WebSocket-Protocol", "wfm".parse().unwrap());
        // TODO: We probably need to require the developer to enter a useragent so not every app using this gets generalized as one and the same
        headers.append("User-Agent", "wf-market-rs".parse().unwrap());
        
        let (ws_stream, _) = connect_async(request).await.map_err(|err| {
            println!("Failed to establish connection to {}: {}", WS_URL, err.to_string());
            match err {
                Error::Http(mut http) => {
                    let body = http.body_mut();
                    for char in body.clone().unwrap() {
                        print!("{}", char as char);
                    }
                    print!("\n")
                },
                _ => unreachable!()
            }
            WsError::ConnectionError
        })?;
        let (mut write, mut read) = ws_stream.split();

        // Create message channel
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let sender = MessageSender { tx };

        // Spawn write task
        let write_task = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&message) {
                    if let Err(e) = write.send(Message::Text(Utf8Bytes::from(json))).await {
                        eprintln!("Failed to send message: {}", e);
                        break;
                    }
                }
            }
        });

        // Call connected callback if registered
        if let Some(connected_callback) = self.router.routes.get("internal/connected") {
            let route = Route {
                protocol: "@internal".to_string(),
                path: "internal/connected".to_string(),
                parameter: None,
            };
            connected_callback(&WsMessage {
                route: "@internal|internal/connected".to_string(),
                payload: Some(serde_json::Value::from(true)),
                id: Some("INTERNAL".to_string()),
                ref_id: None,
            }, &route, &sender)?;
        }

        // Spawn message handling task
        let router = Arc::new(self.router);
        let sender_clone = sender.clone();
        let read_task = tokio::spawn(async move {
            while let Some(message) = read.next().await {
                if let Ok(message) = message {
                    match message {
                        Message::Text(text) => {
                            if let Err(e) = WsClient::handle_text_message(&router, &text, &sender_clone) {
                                eprintln!("Error handling message: {:?}", e);
                            }
                        }
                        Message::Close(_) => { break }
                        _ => { println!("Unexpected message: {:?}", message); }
                    }
                }
            }
        });

        // Return the built client - spawn background task to manage the connection
        tokio::spawn(async move {
            let _ = tokio::join!(read_task, write_task);
        });

        Ok(WsClient {
            sender: Some(sender),
        })
    }
}

// The actual WebSocket client (runtime instance)
pub struct WsClient {
    sender: Option<MessageSender>,
}

impl WsClient {
    /// Create a new WebSocket client builder
    pub fn new() -> WsClientBuilder {
        WsClientBuilder::new()
    }

    pub(crate) fn handle_text_message(router: &Router, text: &str, sender: &MessageSender) -> Result<(), WsError> {
        let message: WsMessage = serde_json::from_str(text).map_err(|_| WsError::InvalidMessageReceived(text.to_string()))?;
        router.route_message(&message, sender)
    }

    // Public methods for sending messages (only available after build)
    pub fn send_message(&self, message: WsMessage) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message(message)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn send_response(
        &self,
        route: &str,
        payload: serde_json::Value,
        ref_id: &str
    ) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_response(route, payload, ref_id)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn send_request(
        &self,
        route: &str,
        payload: serde_json::Value
    ) -> Result<String, WsError> {
        if let Some(sender) = &self.sender {
            sender.send_request(route, payload)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn get_sender(&self) -> Option<MessageSender> {
        self.sender.clone()
    }
}