use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub enum StatusType {
    #[serde(rename = "offline")]
    Offline,
    #[serde(rename = "online")]
    Online,
    #[serde(rename = "in_game")]
    InGame,
}

fn default_status_type() -> StatusType {
    StatusType::Offline
}

#[derive(Clone, Deserialize)]
pub struct FullUser {
    pub id: String,
    #[serde(rename = "ingame_name", alias = "ingameName")]
    pub name: String,
    pub role: String,
    pub reputation: i32,
    pub platform: String,
    #[serde(rename = "status", default = "default_status_type")]
    pub status_type: StatusType,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned: Option<bool>,
    
    #[serde(alias = "unreadNotifications")]
    pub unread_messages: i32,
}

#[derive(Deserialize)]
pub struct MinimalUser {
    pub id: String,
    #[serde(rename = "ingame_name", alias = "ingameName")]
    pub name: String,
    pub role: String,
    pub reputation: i32
}