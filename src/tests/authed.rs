use std::env;
use dotenv::dotenv;
use crate::client::Client;

#[tokio::test]
async fn test_authentication() {
    dotenv().ok();

    let user = env::var("TEST_USER")
        .expect("TEST_USER must be set in .env for integration tests");
    let pass = env::var("TEST_PASS")
        .expect("TEST_PASS must be set in .env for integration tests");

    assert!(!user.is_empty());
    assert!(!pass.is_empty());

    let _client = Client::new();
    _client.login(&user, &pass, "dev").await.unwrap();
}