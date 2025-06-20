/*!
Provides a managed `Riven` object, unlike the [`Riven`][crate::types::riven::Riven] type, provides some helper functions

# Examples

```rust
use wf_market::{
    client::Client,
    utils::generate_device_id,
};

#[tokio::main]
async fn main() {
    let mut client = Client::new();

    match client.get_rivens().await {
        Ok(mut rivens) => {
            println!("Riven Slugs:");
            for riven in rivens {
                println!("{}: {}", riven.get_slug(), riven.get_name());
            }
        },
        Err(e) => println!("Error: {:?}", e),
    }
}
```
*/

use crate::types::riven::Riven as RiveType;

#[derive(Clone)]
pub struct Riven {
    object: RiveType,
}

impl Riven {
    pub fn new(object: &RiveType) -> Self {
        Riven {
            object: object.clone(),
        }
    }
    pub fn get_type(&self) -> RiveType {
        self.object.clone()
    }

    pub fn get_slug(&self) -> String {
        self.object.slug.clone()
    }

    pub fn get_name(&self) -> String {
        if let Some(en) = self.object.i18n.get("en") {
            en.name.clone()
        } else {
            String::new()
        }
    }
}
