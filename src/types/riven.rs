use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq)]
pub enum RivenType {
    #[serde(rename = "kitgun")]
    Kitgun,
    #[serde(rename = "melee")]
    Melee,
    #[serde(rename = "pistol")]
    Pistol,
    #[serde(rename = "rifle")]
    Rifle,
    #[serde(rename = "shotgun")]
    Shotgun,
    #[serde(rename = "zaw")]
    Zaw,
}

#[derive(Clone, Deserialize)]
pub struct Riven {
    pub id: String,
    pub slug: String,

    #[serde(rename = "gameRef", skip_serializing_if = "Option::is_none")]
    pub game_ref: Option<String>,

    #[serde(rename = "rivenType")]
    pub riven_type: RivenType,

    #[serde(rename = "disposition")]
    pub disposition: f64,

    #[serde(rename = "reqMasteryRank")]
    pub req_mastery_rank: i8,

    #[serde(default = "HashMap::new")]
    pub i18n: HashMap<String, RivenTranslation>,
}

#[derive(Clone, Deserialize)]
pub struct RivenTranslation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: String,

    #[serde(rename = "wikiLink", skip_serializing_if = "Option::is_none")]
    pub wiki_link: Option<String>,
    pub icon: String,

    pub thumb: String,
}
