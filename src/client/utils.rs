use serde::Deserialize;
use crate::types::user::FullUser;

#[derive(Deserialize)]
pub(super) struct AuthResp {
    pub(super) user: FullUser,
}

/**
INTERNAL: Build the HTTP client with default settings

# Arguments
- `auth`: Authentication token used when communicating with authenticated endpoints

# Returns
- A `reqwest::Client` with assigned default headers
*/
pub(super) fn build_http(auth: Option<String>) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(auth) = auth {
        headers.insert(reqwest::header::AUTHORIZATION, auth.parse().unwrap());
    }
    headers.insert("language", "en".parse().unwrap());
    headers.insert("platform", "pc".parse().unwrap());

    reqwest::Client::builder()
        .default_headers(headers)
        .build().unwrap()
}