use serde::Deserialize;

#[derive(Copy, Clone, Deserialize)]
pub struct APIV1Result<T> {
    pub payload: T,
}

#[derive(Clone, Deserialize)]
pub struct ApiResult<T> {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub data: T,
}