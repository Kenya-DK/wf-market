use std::sync::Arc;
use futures_util::future::join_all;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::Client;

#[tokio::test]
async fn test_orders() {
    let client: Arc<Mutex<Client>> = Mutex::new(Client::new()).into();
    
    let mut join_handles: Vec<JoinHandle<()>> = Vec::new();
    for _ in 0..10 {
        let client_clone = client.clone();
        let task = tokio::spawn(async move {
            let ret = client_clone.lock().await.get_items().await;
            assert!(ret.is_ok());
        });
        join_handles.push(task);
    }
    
    join_all(join_handles).await;
}