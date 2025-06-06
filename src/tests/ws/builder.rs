use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use crate::client::ws::{WsClient, WsMessage};
use crate::error::WsError;
use crate::tests::ws::builder;

#[tokio::test]
async fn test_build_client() {
    let _ = WsClient::new()
        .register_callback("unused/event", |_,_,_| Ok(())).unwrap()
        .register_callback("unused/event2", |_,_,_| Ok(())).unwrap()
        .build().await.unwrap();
}

#[tokio::test]
async fn test_connection() {
    let received_messages: Arc<Mutex<Vec<WsMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let received_messages_clone1 = received_messages.clone();
    let received_messages_clone2 = received_messages.clone();
    
    let client = WsClient::new()
        .register_callback("internal/connected", move |msg,_,_| {
            let mut arr = received_messages_clone1.lock().unwrap();
            arr.push(msg.clone());

            Ok(())
        }).unwrap()
        .register_callback("event/reports/online", move |msg, _, _| {
            let mut arr = received_messages_clone2.lock().unwrap();
            arr.push(msg.clone());
            
            Ok(())
        }).unwrap()
        .build().await.unwrap();
    
    let time = SystemTime::now();
    while time.elapsed().unwrap() <= Duration::from_secs(5) {
        if received_messages.lock().unwrap().len() > 0 {
            return;
        }
    }
    
    assert!(received_messages.lock().unwrap().len() > 0);
}
