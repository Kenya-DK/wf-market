/*!
WebSocket client module for Warframe Market.

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
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{Error, Message, Utf8Bytes};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use crate::error::WsError;

/// The Warframe Market WebSocket URL for the PC platform.
pub(super) const WS_URL: &str = "wss://warframe.market/socket?platform=pc";

/// The protocol identifier prepended to all message types.
pub(super) const WS_PROTOCOL: &str = "@WS";

/// A generic WebSocket message with a typed “type” and optional JSON payload.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WsMessage {
    /// The message type in the form `"<PROTOCOL>/<PATH>"`, e.g. `"@WS/MESSAGE/ONLINE_COUNT"`.
    #[serde(rename = "type")]
    pub message_type: String,

    /// An optional JSON payload attached to the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// A parsed representation of a WebSocket message’s protocol and path.
///
/// Example:
/// ```
/// let route = Route::parse("@WS/ORDERS/NEW").unwrap();
/// assert_eq!(route.protocol, "@WS");
/// assert_eq!(route.path, "ORDERS/NEW");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    /// The protocol portion of the message type (e.g. `"@WS"`).
    pub protocol: String,

    /// The path portion of the message type (e.g. `"ORDERS/NEW"`).
    pub path: String,
}

impl Route {
    /// Parses a raw type string of the form `"<protocol>/<path>"` into a `Route`.
    ///
    /// # Errors
    /// Returns `WsError::InvalidPath` if the string does not contain a `/`.
    pub fn parse(type_str: &str) -> Result<Self, WsError> {
        if let Some(slash_pos) = type_str.find('/') {
            let protocol = type_str[..slash_pos].to_string();
            let path = type_str[slash_pos + 1..].to_string();
            Ok(Route { protocol, path })
        } else {
            Err(WsError::InvalidPath(type_str.to_string()))
        }
    }

    /// Formats the route back into a single string `"<protocol>/<path>"`.
    pub fn to_string(&self) -> String {
        format!("{}/{}", self.protocol, self.path)
    }

    /// Returns the path component of the route.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// A handle for sending `WsMessage`s into the WebSocket write loop.
#[derive(Clone)]
pub struct MessageSender {
    tx: mpsc::UnboundedSender<WsMessage>,
}

impl MessageSender {
    /// Sends a raw `WsMessage` into the outgoing channel.
    ///
    /// # Errors
    /// Returns `WsError::SendError` if the channel is closed.
    pub fn send_message(&self, message: WsMessage) -> Result<(), WsError> {
        self.tx.send(message).map_err(|_| WsError::SendError)?;
        Ok(())
    }

    /// Sends a message to a specific path with a JSON payload.
    ///
    /// Convenience wrapper for sending `@WS/<path>` messages.
    ///
    /// # Errors
    /// Returns `WsError::SendError` if the channel is closed.
    pub fn send_message_to_path(
        &self,
        path: &str,
        payload: serde_json::Value,
    ) -> Result<(), WsError> {
        let message = WsMessage {
            message_type: format!("{}/{}", WS_PROTOCOL, path),
            payload: Some(payload),
        };
        self.send_message(message)
    }

    /// Sends a message to a specific path without any payload.
    ///
    /// # Errors
    /// Returns `WsError::SendError` if the channel is closed.
    pub fn send_message_to_path_no_payload(&self, path: &str) -> Result<(), WsError> {
        let message = WsMessage {
            message_type: format!("{}/{}", WS_PROTOCOL, path),
            payload: None,
        };
        self.send_message(message)
    }
}

/// A thread-safe callback type for handling incoming WebSocket messages.
///
/// The callback receives:
/// - a reference to the parsed `WsMessage`,
/// - the extracted `Route`,
/// - and a `MessageSender` for replying or sending new messages.
pub type MessageCallback =
Arc<dyn Fn(&WsMessage, &Route, &MessageSender) -> Result<(), WsError> + Send + Sync>;

/// Internal router that maps message paths to their registered callbacks.
pub(crate) struct Router {
    routes: HashMap<String, MessageCallback>,
}

impl Router {
    /// Creates an empty router.
    fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Lists internal paths reserved for client lifecycle events.
    fn get_reserved_paths() -> Vec<&'static str> {
        vec!["CONNECTION/ESTABLISHED"]
    }

    /// Checks if a path is reserved by the client.
    fn is_path_reserved(path: &str) -> bool {
        Self::get_reserved_paths().contains(&path)
    }

    /// Registers a callback for a given path.
    ///
    /// # Errors
    /// - `WsError::ReservedPath` if the path is reserved internally.
    /// - `WsError::AlreadyRegistered` if a callback is already registered.
    fn register(&mut self, path: &str, callback: MessageCallback) -> Result<(), WsError> {
        if Self::is_path_reserved(path) {
            return Err(WsError::ReservedPath(path.to_string()));
        }
        if self.routes.contains_key(path) {
            return Err(WsError::AlreadyRegistered(path.to_string()));
        }
        self.routes.insert(path.to_string(), callback);
        Ok(())
    }

    /// Routes an incoming `WsMessage` to the appropriate callback.
    ///
    /// Filters by protocol, handles internal paths first, then user-registered paths.
    fn route_message(&self, message: &WsMessage, sender: &MessageSender) -> Result<(), WsError> {
        let route = Route::parse(&message.message_type)?;

        if route.protocol != WS_PROTOCOL {
            // Ignore messages from other protocols.
            println!("Ignoring message with different protocol: {}", route.protocol);
            return Ok(());
        }

        if Self::is_path_reserved(route.path()) {
            return self.handle_internal_route(&route, message, sender);
        }

        if let Some(callback) = self.routes.get(route.path()) {
            callback(message, &route, sender)?;
        } else {
            println!("No handler for route: {}", route.path());
        }

        Ok(())
    }

    /// Handles reserved internal routes (e.g. connection life-cycle events).
    fn handle_internal_route(
        &self,
        route: &Route,
        _message: &WsMessage,
        _sender: &MessageSender,
    ) -> Result<(), WsError> {
        match route.path() {
            "CONNECTION/ESTABLISHED" => {
                println!("Connection established");
            }
            other => {
                println!("Unhandled internal route: {}", other);
            }
        }
        Ok(())
    }
}

/// Builder for configuring and launching a `WsClient`.
pub struct WsClientBuilder {
    router: Router,
    token: String,
}

impl WsClientBuilder {
    /// Creates a new builder given an authentication JWT token.
    pub(crate) fn new(token: String) -> Self {
        Self {
            router: Router::new(),
            token,
        }
    }

    /// Registers a callback for a specific message path.
    ///
    /// # Examples
    ///
    /// ```
    /// builder = builder.register_callback("MESSAGE/ONLINE_COUNT", |msg, _route, _sender| {
    ///     println!("Online: {}", msg.payload.unwrap()["authorizedUsers"]);
    ///     Ok(())
    /// })?;
    /// ```
    ///
    /// # Errors
    /// - `WsError::ReservedPath` if the path is internal
    /// - `WsError::AlreadyRegistered` if the path already has a callback
    pub fn register_callback<F>(mut self, path: &str, callback: F) -> Result<Self, WsError>
    where
        F: Fn(&WsMessage, &Route, &MessageSender) -> Result<(), WsError>
        + Send
        + Sync
        + 'static,
    {
        self.router.register(path, Arc::new(callback))?;
        Ok(self)
    }

    /// Returns the list of client-reserved paths.
    pub fn get_reserved_paths() -> Vec<&'static str> {
        Router::get_reserved_paths()
    }

    /// Builds and starts the WebSocket client, returning a running `WsClient`.
    ///
    /// This will:
    /// 1. Open the WebSocket connection
    /// 2. Spawn a task to send outbound messages
    /// 3. Spawn a task to read inbound messages and route them
    /// 4. Invoke any registered `CONNECTION/ESTABLISHED` callback
    ///
    /// # Errors
    /// Returns `WsError::ConnectionError` if the connection fails.
    pub async fn build(self) -> Result<WsClient, WsError> {
        let mut request = WS_URL.into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.append("Cookie", format!("JWT={}", self.token).parse().unwrap());
        headers.append("User-Agent", "wf-market-rs".parse().unwrap());

        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|_| WsError::ConnectionError)?;
        let (mut write, mut read) = ws_stream.split();

        // Channel for outgoing messages
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let sender = MessageSender { tx };

        // Task: write loop
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

        // Invoke connection-established callback if present
        if let Some(cb) = self.router.routes.get("CONNECTION/ESTABLISHED") {
            let route = Route {
                protocol: WS_PROTOCOL.to_string(),
                path: "CONNECTION/ESTABLISHED".to_string(),
            };
            cb(
                &WsMessage {
                    message_type: format!("{}/CONNECTION/ESTABLISHED", WS_PROTOCOL),
                    payload: Some(serde_json::json!({ "connected": true })),
                },
                &route,
                &sender,
            )?;
        }

        // Task: read loop
        let router = Arc::new(self.router);
        let sender_clone = sender.clone();
        let read_task = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Err(e) =
                        WsClient::handle_text_message(&router, &text, &sender_clone)
                    {
                        eprintln!("Error handling message: {:?}", e);
                    }
                }
            }
        });

        // Detach tasks
        tokio::spawn(async move {
            let _ = tokio::join!(write_task, read_task);
        });

        Ok(WsClient {
            sender: Some(sender),
        })
    }
}

/// A live WebSocket client instance that can send messages after connection.
pub struct WsClient {
    sender: Option<MessageSender>,
}

impl WsClient {
    /// Parses an inbound text message and routes it via the provided `Router`.
    ///
    /// # Errors
    /// Returns `WsError::InvalidMessageReceived` if JSON deserialization fails.
    pub(crate) fn handle_text_message(
        router: &Router,
        text: &str,
        sender: &MessageSender,
    ) -> Result<(), WsError> {
        let message: WsMessage = serde_json::from_str(text)
            .map_err(|_| WsError::InvalidMessageReceived(text.to_string()))?;
        router.route_message(&message, sender)
    }

    /// Sends a raw `WsMessage` if the client is connected.
    ///
    /// # Errors
    /// Returns `WsError::NotConnected` if `build()` has not been called.
    pub fn send_message(&self, message: WsMessage) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message(message)
        } else {
            Err(WsError::NotConnected)
        }
    }

    /// Sends a message to `@WS/<path>` with a JSON payload.
    pub fn send_message_to_path(
        &self,
        path: &str,
        payload: serde_json::Value,
    ) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message_to_path(path, payload)
        } else {
            Err(WsError::NotConnected)
        }
    }

    /// Sends a message to `@WS/<path>` without payload.
    pub fn send_message_to_path_no_payload(&self, path: &str) -> Result<(), WsError> {
        if let Some(sender) = &self.sender {
            sender.send_message_to_path_no_payload(path)
        } else {
            Err(WsError::NotConnected)
        }
    }

    /// Retrieves a clone of the internal `MessageSender` for custom send workflows.
    pub fn get_sender(&self) -> Option<MessageSender> {
        self.sender.clone()
    }
}