use std::collections::HashMap;
use serde::Deserialize;
use crate::types::user::{MinimalUser};

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq)]
pub enum OrderType {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Order {
    pub id: String,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub platinum: u32, // AKA the price
    pub quantity: u32,
    
    #[serde(rename = "perTrade", skip_serializing_if = "Option::is_none")]
    pub per_trade: Option<u8>, // Amount of items per trade
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>, // Subtype of the item, if applicable
    
    // MODS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>, // Rank of the mod, if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charges: Option<u8>, // Charges remaining (Requiem mods)
    
    // AYATAN SCULPTURES
    #[serde(rename = "amberStars", skip_serializing_if = "Option::is_none")]
    pub amber_stars: Option<u8>, // Number of Amber Stars, if applicable
    #[serde(rename = "cyanStars", skip_serializing_if = "Option::is_none")]
    pub cyan_stars: Option<u8>, // Number of Cyan Stars, if applicable
    
    pub visible: bool, // Whether the order is visible to other players
    
    #[serde(rename = "itemId")]
    pub item_id: String, // ID of the item
    
    #[serde(rename = "createdAt")]
    pub created_at: String, // Timestamp of when the order was created
    #[serde(rename = "updatedAt")]
    pub updated_at: String, // Timestamp of when the order was last updated
}

#[derive(Clone, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(default = "Vec::new")]
    pub tags: Vec<String>,
    pub slug: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaulted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ducats: Option<u32>,
    
    // MODS
    #[serde(rename = "maxRank", skip_serializing_if = "Option::is_none")]
    pub max_rank: Option<u32>,
    #[serde(rename = "maxCharges", skip_serializing_if = "Option::is_none")]
    pub max_charges: Option<u32>,
    
    // AYATAN SCULPTURES
    #[serde(rename = "maxAmberStars", skip_serializing_if = "Option::is_none")]
    pub max_amber_stars: Option<u32>,
    #[serde(rename = "maxCyanStars", skip_serializing_if = "Option::is_none")]
    pub max_cyan_stars: Option<u32>,
    #[serde(rename = "baseEndo", skip_serializing_if = "Option::is_none")]
    pub base_endo: Option<u32>,
    #[serde(rename = "endoMultiplier", skip_serializing_if = "Option::is_none")]
    pub endo_multiplier: Option<f32>,
    
    #[serde(rename = "reqMasteryRank", skip_serializing_if = "Option::is_none")]
    pub mastery_rank: Option<u32>,
    #[serde(default = "HashMap::new")]
    pub i18n: HashMap<String, ItemTranslation>
}

#[derive(Clone, Deserialize)]
pub struct ItemTranslation {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "wikiLink", skip_serializing_if = "Option::is_none")]
    pub wiki_link: Option<String>,
    pub icon: String,
}

#[derive(Debug, Deserialize)]
pub struct OrderWithUser {
    #[serde(flatten)]
    pub order: Order,

    pub user: MinimalUser,
}

impl OrderWithUser {
    pub fn downgrade(&self) -> Order {
        let o = self.order.clone();
        Order {
            id: o.id,
            order_type: o.order_type,
            platinum: o.platinum,
            quantity: o.quantity,
            per_trade: o.per_trade,
            subtype: o.subtype,
            rank: o.rank,
            charges: o.charges,
            amber_stars: o.amber_stars,
            cyan_stars: o.cyan_stars,
            visible: o.visible,
            created_at: o.created_at,
            updated_at: o.updated_at,
            item_id: o.item_id,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OrdersTopResult {
    pub buy: Vec<OrderWithUser>,
    pub sell: Vec<OrderWithUser>,
}
