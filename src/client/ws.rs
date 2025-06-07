/*!
# Websockets

Build a WebSocket Client to receive real-time information from Warframe Market

## Record active users
```rust
use wf_market::{
    error::WsError,
    client::ws::WsClient,
    Client
};

#[tokio::main]
async fn main() -> Result<(), WsError> {
    let mut client = {
        Client::new()
            .login("user", "pass", "dev").await.unwrap()
    };

    let client = client.create_websocket()
        .register_callback("MESSAGE/ONLINE_COUNT", |msg, _, _| {
            let payload = msg.payload.clone().unwrap();
            println!("Users Online: {}", payload.get("authorizedUsers").unwrap().as_i64());
            Ok(())
        })?
        .build().await?;

    tokio::signal::ctrl_c().await.unwrap()
}
```
*/

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async};
use tokio_tungstenite::tungstenite::{Error, Message, Utf8Bytes};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use crate::error::WsError;

pub(super) const WS_URL: &'static str = "wss://warframe.market/socket?platform=pc";
pub(super) const WS_PROTOCOL: &'static str = "@WS";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WsMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

// Route structure for the new format
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    pub protocol: String,
    pub path: String,
}

impl Route {
    pub fn parse(type_str: &str) -> Result<Self, WsError> {
        if let Some(slash_pos) = type_str.find('/') {
            let protocol = type_str[..slash_pos].to_string();
            let path = type_str[slash_pos + 1..].to_string();
            Ok(Route { protocol, path })
        } else {
            Err(WsError::InvalidPath(type_str.to_string()))
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}/{}", self.protocol, self.path)
    }

    pub fn path(&self) -> &str {
        &self.path
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

    pub fn send_message_to_path(
        &self,
        path: &str,
        payload: serde_json::Value
    ) -> Result<(), WsError> {
        let message = WsMessage {
            message_type: format!("{}/{}", WS_PROTOCOL, path),
            payload: Some(payload),
        };
        self.send_message(message)
    }

    pub fn send_message_to_path_no_payload(&self, path: &str) -> Result<(), WsError> {
        let message = WsMessage {
            message_type: format!("{}/{}", WS_PROTOCOL, path),
            payload: None,
        };
        self.send_message(message)
    }
}

// Updated callback type
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
            "CONNECTION/ESTABLISHED",
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
        let route = Route::parse(&message.message_type)?;

        // Only handle messages with our protocol
        if route.protocol != WS_PROTOCOL {
            println!("Ignoring message with different protocol: {}", route.protocol);
            return Ok(());
        }

        // Handle internal routes first
        if Self::is_path_reserved(route.path()) {
            self.handle_internal_route(&route, message, sender)?;
            return Ok(());
        }

        // Try to find callback for the path
        if let Some(callback) = self.routes.get(route.path()) {
            callback(message, &route, sender)?;
        } else {
            // Optionally log unhandled routes
            println!("No handler for route: {}", route.path());
        }

        Ok(())
    }

    // Handle internal client routes
    fn handle_internal_route(&self, route: &Route, message: &WsMessage, sender: &MessageSender) -> Result<(), WsError> {
        match route.path() {
            "CONNECTION/ESTABLISHED" => {
                println!("Connection established");
            }
            _ => {
                println!("Unhandled internal route: {}", route.path());
            }
        }
        Ok(())
    }
}

// WebSocket client builder
pub struct WsClientBuilder {
    router: Router,
    token: String,
}

impl WsClientBuilder {
    pub(crate) fn new(token: String) -> Self {
        Self {
            router: Router::new(),
            token,
        }
    }

    /// Register a callback for a specific path
    ///
    /// Examples:
    /// - `register_callback("USER/SET_STATUS", callback)` - handles @WS/USER/SET_STATUS messages
    /// - `register_callback("ORDERS/NEW", callback)` - handles @WS/ORDERS/NEW messages
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
        // headers.append("Sec-WebSocket-Protocol", "wfm".parse().unwrap());
        let cookie = format!("JWT={}", self.token);
        headers.append("Cookie", cookie.parse().unwrap());
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
                _ => println!("{:?}", err)
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
        if let Some(connected_callback) = self.router.routes.get("CONNECTION/ESTABLISHED") {
            let route = Route {
                protocol: WS_PROTOCOL.to_string(),
                path: "CONNECTION/ESTABLISHED".to_string(),
            };
            connected_callback(&WsMessage {
                message_type: format!("{}/CONNECTION/ESTABLISHED", WS_PROTOCOL),
                payload: Some(serde_json::json!({"connected": true})),
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

    pub fn send_message_to_path(
        &self,
        path: &str,
        payload: serde_json::Value
    ) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message_to_path(path, payload)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn send_message_to_path_no_payload(&self, path: &str) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message_to_path_no_payload(path)
        } else {
            Err(WsError::NotConnected)
        }
    }

    pub fn get_sender(&self) -> Option<MessageSender> {
        self.sender.clone()
    }
}
