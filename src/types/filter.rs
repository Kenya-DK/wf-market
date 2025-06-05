use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct OrdersTopFilters {
    pub rank: u32,
    #[serde(rename = "rankLt")]
    pub rank_lt: u32,
    
    pub charges: u32,
    #[serde(rename = "chargesLt")]
    pub charges_lt: u32,

    #[serde(rename = "amberStars")]
    pub amber_stars: u32,
    #[serde(rename = "amberStarsLt")]
    pub amber_stars_lt: u32,

    #[serde(rename = "cyanStars")]
    pub cyan_stars: u32,
    #[serde(rename = "cyanStarsLt")]
    pub cyan_stars_lt: u32,
    
    pub subtype: String
}