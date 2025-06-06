/*!
# wf-market
The `wf-market` crate provides an abstraction on top of the **Warframe Market API**

It handles a lot of actions you'd like to perform as trader.
- A simple to use async [`Client`]
- Abstracted object to easily manage both [`Items`][client::item::Item] and [`Orders`][client::order::Order]
- *(COMING SOON)* WebSocket support to keep data up to date

## Constructing and Authenticating a client
```rust
use wf_market::{
    Client,
    utils::generate_device_id,
    client::{
        Unauthenticated,
        Authenticated
    },
};

#[tokio::main]
async fn main() {
    let client: Client<Unauthenticated> = Client::new(); // Client can already now be used

    let authenticated_client: Client<Authenticated> = {
        Client::new()
            .login("username", "password", generate_device_id().as_str())
    };
}
```
NOTE: Not reusing the device_id may generate multiple devices on a user's device

## Find the price of Ayatan Sculptures
```rust
use wf_market::{
    client::Client,
    utils::generate_device_id,
};

#[tokio::main]
async fn main() {
    let mut client = Client::new();

    match client.get_items().await {
        Ok(mut items) => {
            items = items.iter()
                        .filter(|item| item.is_sculpture())
                        .collect();
            println!("Sculpture Valuation:");
            for item in items {
                let sculpture = item.to_sculpture().unwrap();
                println!("{}: {} endo", 
                    sculpture.get_name(), 
                    sculpture.calculate_value(None, None));
            }
        },
        Err(e) => println!("Error: {:?}", e),
    }
}
```
*/

pub mod types;
pub mod error;
pub mod client;
pub mod utils;

mod oauth;

pub use client::Client;

#[cfg(test)]
mod tests;
