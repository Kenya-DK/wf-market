use std::env;
use dotenv::dotenv;
use crate::client::{Authenticated, Client};
use crate::error::AuthError;

async fn setup_client() -> Result<Client<Authenticated>, AuthError> {
    dotenv().ok();

    let user = env::var("TEST_USER")
        .expect("TEST_USER must be set in .env for integration tests");
    let pass = env::var("TEST_PASS")
        .expect("TEST_PASS must be set in .env for integration tests");

    assert!(!user.is_empty());
    assert!(!pass.is_empty());

    let _client = Client::new();
    _client.login(&user, &pass, "dev").await
}

// This is basically tested within my_orders
/*#[tokio::test]
async fn test_authentication() {
    setup_client().await.unwrap();
}*/

#[tokio::test]
async fn test_my_orders() {
    let mut client = setup_client().await.unwrap();

    client.my_orders().await.unwrap();
}
