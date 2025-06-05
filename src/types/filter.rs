use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct OrdersTopFilters {
    pub rank: Option<u32>,
    #[serde(rename = "rankLt")]
    pub rank_lt: Option<u32>,
    
    pub charges: Option<u32>,
    #[serde(rename = "chargesLt")]
    pub charges_lt: Option<u32>,

    #[serde(rename = "amberStars")]
    pub amber_stars: Option<u32>,
    #[serde(rename = "amberStarsLt")]
    pub amber_stars_lt: Option<u32>,

    #[serde(rename = "cyanStars")]
    pub cyan_stars: Option<u32>,
    #[serde(rename = "cyanStarsLt")]
    pub cyan_stars_lt: Option<u32>,
    
    pub subtype: Option<String>
}