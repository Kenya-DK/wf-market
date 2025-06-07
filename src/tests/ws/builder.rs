use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use dotenv::dotenv;
use tokio::time::{sleep, timeout};
use crate::Client;
use crate::client::ws::{WsMessage};

#[tokio::test]
async fn test_connection() {
    let received_messages: Arc<Mutex<Vec<WsMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let received_messages_clone = received_messages.clone();

    dotenv().ok();

    let user = env::var("TEST_USER")
        .expect("TEST_USER must be set in .env for integration tests");
    let pass = env::var("TEST_PASS")
        .expect("TEST_PASS must be set in .env for integration tests");

    assert!(!user.is_empty());
    assert!(!pass.is_empty());

    let mut client = {
        Client::new()
            .login(&user, &pass, "dev").await.unwrap()
    };
    
    let ws_client = client.create_websocket()
        .register_callback("MESSAGE/ONLINE_COUNT", move |msg, _, _| {
            let mut arr = received_messages_clone.lock().unwrap();
            arr.push(msg.clone());
            
            Ok(())
        }).unwrap()
        .build().await.unwrap();
    
    let msg = WsMessage {
        message_type: "@WS/USER/SET_STATUS".to_string(),
        payload: Some(serde_json::Value::String("invisible".to_string())),
    };
    ws_client.send_message(msg);

    let res = timeout(Duration::from_secs(5), async {
        loop {
            {
                let guard = received_messages.lock().unwrap();
                if !guard.is_empty() {
                    break;
                }
            }
            // yield back to Tokio, let the writer+reader run
            sleep(Duration::from_millis(10)).await;
        }
    }).await;
    
    assert!(received_messages.lock().unwrap().len() > 0);
}
