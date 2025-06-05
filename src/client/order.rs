/*!
Provides a managed `Order` object, unlike the Order type provides some helper functions

# Examples

```
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

    client.orders.iter().map(|mut order| {
        let o = order.get_type();
        println!("{} (x{}): {}p", o.item_id, o.quantity, o.platinum);
    })
}
* ```
*/

use std::marker::PhantomData;
use crate::types::item::{Order as OrderItem};

pub struct Owned;
#[derive(Clone)]
pub struct Unowned;

#[derive(Clone)]
pub struct Order<State = Unowned> {
    pub(crate) object: OrderItem,
    _state: PhantomData<State>,
}

impl<State> Order<State> {
    pub fn get_type(&mut self) -> OrderItem {
        self.object.clone()
    }
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
