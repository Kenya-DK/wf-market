use serde::Serialize;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdateParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platinum: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_trade: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}