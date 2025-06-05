use std::collections::HashMap;
use std::marker::PhantomData;
use serde::Serialize;
use crate::client::http::{Method};
use crate::client::utils::{build_http, AuthResp};
use crate::error::{ApiError, AuthError};
use crate::types::http::{APIV1Result, ApiResult};
use crate::types::user::{FullUser, StatusType};
use crate::types::user::StatusType::Offline;

pub struct Unauthenticated;
pub struct Authenticated;

pub struct Client<State = Unauthenticated> {
    pub(crate) http: reqwest::Client,
    /// Current logged in user, updated from `client.refresh()`
    pub user: Option<FullUser>,
    /// Status of the logged in user, updated via WebSocket
    pub status: StatusType,
    _state: PhantomData<State>,
}

pub(super) const BASE_URL: &str = "https://api.warframe.market/v2";
pub(super) const V1_API: &str = "https://api.warframe.market/v1";

#[derive(Serialize)]
struct NoBody;

// Generic implementations (can be used with or without auth)
impl<State> Client<State> {
    
}

impl Client<Unauthenticated> {
    pub fn new() -> Self {
        Client {
            http: build_http(None),
            user: None,
            status: Offline,
            _state: PhantomData,
        }
    }

    pub async fn login(self, username: &str, password: &str, device_id: &str) -> Result<Client<Authenticated>, AuthError> {
        let mut map = HashMap::new();
        map.insert("auth_type", "header");
        map.insert("email", username);
        map.insert("password", password);
        map.insert("device_id", device_id);

        match self.http.post(V1_API.to_owned() + "/auth/signin")
            .json(&map)
            .header("Authorization", "JWT")
            .send()
            .await {
            Ok(resp) => {
                let headers = resp.headers().clone();
                let body = resp.text().await.unwrap();

                let data: APIV1Result<AuthResp> = serde_json::from_str(&body).map_err(|_| AuthError::ParsingError)?;

                match headers.get("Authorization") {
                    Some(header) => {
                        let mut token: String = header.to_str().map_err(|_| AuthError::ParsingError)?.to_string();
                        token = token.replace("JWT", "Bearer");

                        let http = build_http(Some(token));

                        Ok(Client {
                            http,
                            user: Some(data.payload.user.clone()),
                            status: data.payload.user.status_type,
                            _state: PhantomData,
                        })
                    }
                    None => Err(AuthError::ParsingError),
                }
            }
            Err(e) => {
                Err(AuthError::Unknown(format!("Unknown Error: {:?}", e)))
            }
        }
    }
}

impl Client<Authenticated> {
    pub async fn refresh<'a>(mut self) -> Result<FullUser, ApiError> {
        let res: Result<ApiResult<FullUser>, ApiError> = self.call_api(Method::Get, "/me", None::<&NoBody>).await;
        Ok(res?.data)
    }
}