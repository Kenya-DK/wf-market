/*!
Provides a managed `Order` object, unlike the [`Order`][crate::types::item::Order] type provides some helper functions

# Examples

```rust
use wf_market::{
    client::Client,
    utils::generate_device_id,
};

#[tokio::main]
async fn main() {
    let client = {
        Client::new()
            .login("username", "password", generate_device_id().as_str()).await.unwrap()
    };

    println!("My orders:");
    client.orders.iter().map(|mut order| {
        let o = order.get_type();
        println!("{} (x{}): {}p", o.item_id, o.quantity, o.platinum);
    })
}
```
*/

use std::marker::PhantomData;
use chrono::{DateTime, Utc};
use crate::types::item::{Order as OrderItem, OrderType};

pub struct Owned;
#[derive(Clone)]
pub struct Unowned;

#[derive(Clone)]
pub struct Order<State = Unowned> {
    pub(crate) object: OrderItem,
    _state: PhantomData<State>,
}

impl<State> Order<State> {
    pub fn get_id(&self) -> String { self.object.id.clone() }
    pub fn get_type(&self) -> OrderItem {
        self.object.clone()
    }
    pub fn get_platinum(&self) -> u32 { self.object.platinum }
    pub fn updated_at(&self) -> DateTime<Utc> { self.object.updated_at.parse().unwrap_or_default() }
    pub fn created_at(&self) -> DateTime<Utc> { self.object.created_at.parse().unwrap_or_default() }
    pub fn get_visible(&self) -> bool { self.object.visible }
    pub fn get_sell_type(&self) -> OrderType { self.object.order_type }
}

impl Order<Unowned> {
    pub(super) fn new(order: &OrderItem) -> Self {
        Order {
            object: order.clone(),
            _state: PhantomData,
        }
    }
}

impl Order<Owned> {
    pub(super) fn new_owned(order: &OrderItem) -> Self {
        Order {
            object: order.clone(),
            _state: PhantomData,
        }
    }
}
