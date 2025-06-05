use crate::client::Client;
use crate::types::filter::OrdersTopFilters;

const TEST_ITEM: &str = "yareli_prime_set";
const TEST_SCULPTURE: &str = "ayatan_ayr_sculpture";
const SCULPTURE_VALUE: u32 = 1425;
const TEST_MOD: &str = "primed_flow";

#[tokio::test]
async fn test_orders() {
    let mut client = Client::new();
    
    let _ = client.get_orders(TEST_ITEM).await.unwrap();
}

#[tokio::test]
async fn test_orders_top() {
    let mut client = Client::new();
    
    let _ = client.get_orders_top(TEST_ITEM, None).await.unwrap();
}

#[tokio::test]
async fn test_filtered_orders_top() {
    let mut client = Client::new();
    
    let filters = OrdersTopFilters {
        rank: Some(10),
        rank_lt: None,
        charges: None,
        charges_lt: None,
        amber_stars: None,
        amber_stars_lt: None,
        cyan_stars: None,
        cyan_stars_lt: None,
        subtype: None,
    };

    let mods = client.get_orders_top(TEST_MOD, Some(filters)).await.unwrap();
    let _ = mods.iter().map(|order| {
        let item_mod = order.clone().get_type();
        assert_eq!(item_mod.rank.unwrap(), 10u8);
    });
}

#[tokio::test]
async fn all_items() {
    let mut client = Client::new();

    let _ = client.get_items().await.unwrap();
}

#[tokio::test]
async fn get_item() {
    let mut client = Client::new();

    let _ = client.get_item(TEST_ITEM).await.unwrap();
}

#[tokio::test]
async fn convert_mod() {
    let mut client = Client::new();
    let items = client.get_items().await.unwrap();
    
    if let Some(item) = items.iter().find(|i| i.get_slug() == TEST_MOD) {
        match item.is_mod() {
            true => {
                let _ = item.to_mod().unwrap();
            }
            false => {
                panic!("{} is not a mod", item.get_slug());
            }
        }
    }
}

#[tokio::test]
async fn convert_sculpture() {
    let mut client = Client::new();
    let items = client.get_items().await.unwrap();

    if let Some(item) = items.iter().find(|i| i.get_slug() == TEST_SCULPTURE) {
        match item.is_sculpture() {
            true => {
                let sculpture = item.to_sculpture().unwrap();
                let value = sculpture.calculate_value(None, None);
                
                assert_eq!(value, SCULPTURE_VALUE);
            }
            false => {
                panic!("{} is not a sculpture", item.get_slug());
            }
        }
    }
}
