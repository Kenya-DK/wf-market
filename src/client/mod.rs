/*!
Provides `Client` struct for interacting with the Warframe Market API.

# Examples

Running unauthenticated:
```rust
use wf_market::client::Client;

let client = Client::new();
```

Running authenticated:
```rust
use wf_market::{
    client::Client,
    utils::generate_device_id,
};

async fn main() {
    let client = {
        // device_id should be stored and reused
        Client::new()
            .login("username", "password", generate_device_id().as_str()).await.unwrap()
    };
    
    let user = client.user.unwrap();
    println!("Logged in as: {}", user.name);
}
```
*/

mod client;
pub(super) mod utils;
pub(super) mod http;

pub mod order;
mod item;

pub use client::*;