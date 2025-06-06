use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async};
use tokio_tungstenite::tungstenite::Message;
use crate::error::{ WsError};

pub(super) const WS_URL: &'static str = "wss://warframe.market/socket-v2";

#[derive(Debug, Deserialize, Serialize)]
pub struct WsMessage {
    pub route: String,
    pub payload: serde_json::Value,
    pub id: String,
    #[serde(rename = "refId")]
    pub ref_id: Option<String>,
}

// Route structure
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    pub protocol: String,
    pub path: String,
}

impl Route {
    pub fn parse(route_str: &str) -> Result<Self, WsError> {
        if let Some(pipe_pos) = route_str.find('|') {
            let protocol = route_str[..pipe_pos].to_string();
            let path = route_str[pipe_pos + 1..].to_string();
            Ok(Route { protocol, path })
        } else {
            Err(WsError::InvalidPath(route_str.to_string()))
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}|{}", self.protocol, self.path)
    }
}

// Callback type
pub type MessageCallback = Arc<dyn Fn(&WsMessage) -> Result<(), WsError> + Send + Sync>;

// Internal router
struct Router {
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

    fn route_message(&self, message: &WsMessage) -> Result<(), WsError> {
        let route = Route::parse(&message.route)?;

        // Handle internal routes first
        if Self::is_path_reserved(&route.path) {
            self.handle_internal_route(&route.path, message)?;
            return Ok(());
        }

        // Route to user-registered callbacks
        if let Some(callback) = self.routes.get(&route.path) {
            callback(message)?;
        } else {
            // Optionally log unhandled routes
            println!("No handler for route: {}", route.path);
        }

        Ok(())
    }

    // Handle internal client routes
    fn handle_internal_route(&self, path: &str, message: &WsMessage) -> Result<(), WsError> {
        match path {
            _ => {
                println!("Unhandled internal route: {}", path);
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
    pub fn new() -> Self {
        Self {
            router: Router::new(),
        }
    }

    pub fn register_callback<F>(&mut self, path: &str, callback: F) -> Result<(), WsError>
    where
        F: Fn(&WsMessage) -> Result<(), WsError> + Send + Sync + 'static,
    {
        self.router.register(path, Arc::new(callback))
    }

    /// Get list of paths reserved by the client for internal usage
    pub fn get_reserved_paths() -> Vec<&'static str> {
        Router::get_reserved_paths()
    }

    pub async fn run(self) -> Result<(), WsError> {
        let ws_client = WsClient::new(self.router);
        ws_client.start().await
    }
}

// The actual WebSocket client
pub struct WsClient {
    router: Router,
}

impl WsClient {
    fn new(router: Router) -> Self {
        Self { router }
    }

    pub async fn start(self) -> Result<(), WsError> {
        let (ws_stream, _) = connect_async(WS_URL).await.map_err(|_| WsError::ConnectionError)?;
        let (mut write, mut read) = ws_stream.split();
        
        if let Some(connected_callback) = self.router.routes.get("internal/connected") {
            connected_callback(&WsMessage {
                route: "@internal|internal/connected".parse().unwrap(),
                payload: serde_json::Value::from(true),
                id: "INTERNAL".parse().unwrap(),
                ref_id: None,
            })?;
        }
        
        // Message handling loop
        while let Some(message) = read.next().await {
            if let Ok(message) = message {
                match message {
                    Message::Text(text) => {
                        if let Err(e) = self.handle_text_message(&text) {
                            eprintln!("Error handling message: {:?}", e);
                        }
                    }
                    Message::Close(_) => { break }
                    _ => { println!("Unexpected message: {:?}", message); }
                }
            }
        }

        Ok(())
    }

    fn handle_text_message(&self, text: &str) -> Result<(), WsError> {
        let message: WsMessage = serde_json::from_str(text).map_err(|_| WsError::InvalidMessageReceived(text.to_string()))?;
        self.router.route_message(&message)
    }
}
