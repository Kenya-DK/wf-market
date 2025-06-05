use std::collections::HashMap;
use std::marker::PhantomData;
use serde::Serialize;
use crate::client::http::{Method};
use crate::client::utils::{build_http, AuthResp};
use crate::error::{ApiError, AuthError};
use crate::types::http::{APIV1Result, ApiResult};
use crate::client::order::{Order, Owned, Unowned};
use crate::types::user::{FullUser, StatusType};
use crate::types::user::StatusType::Offline;
use crate::types::item::Order as OrderItem;

pub struct Unauthenticated;
pub struct Authenticated;

pub struct Client<State = Unauthenticated> {
    pub(crate) http: reqwest::Client,
    /// Current logged in user, updated from `client.refresh()`
    pub user: Option<FullUser>,
    /// Orders of the logged in user
    pub orders: Vec<Order<Owned>>,
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
            orders: Vec::new(),
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
                        
                        let mut authed_client = Client {
                            http,
                            user: Some(data.payload.user.clone()),
                            orders: Vec::new(),
                            status: data.payload.user.status_type,
                            _state: PhantomData,
                        };

                        authed_client.refresh().await
                            .map_err(|_| AuthError::Unknown("Unable to refresh user after authentication".to_string()))?;

                        Ok(authed_client)
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
    /**
    Refresh the users data, updates the state of `orders` and `user` 
    
    # Returns
    - A FullUser object
    */
    pub async fn refresh<'a>(&mut self) -> Result<FullUser, ApiError> {
        let user: Result<ApiResult<FullUser>, ApiError> = 
            self.call_api(Method::Get, "/me", None::<&NoBody>).await;
        let orders: Result<ApiResult<Vec<OrderItem>>, ApiError> = 
            self.call_api(Method::Get, "/orders/my", None::<&NoBody>).await;
        
        let order_instances = 
            orders?.data.iter().map(|order| Order::new_owned(order)).collect();
        let user_data = user?.data;
        
        self.orders = order_instances;
        self.user = Some(user_data.clone());
        
        Ok(user_data)
    }
    
    /**
    Take ownership of an order, converts an <Unowned> order to an <Owned> one
    
    # Note
    This is using the stored information from the last `.refresh()`, 
    without a WebSocket connection this may be out of date unless manually updated
    
    # Arguments
    - `order`: Managed Order object
    
    # Returns
    - An Owned order
    */
    pub fn take_order(self, order: Order<Unowned>) -> Result<Order<Owned>, ApiError> {
        if let Some(users_order) = self.orders.iter().find(|_order| _order.order.id == order.order.id) {
            Ok(Order::new_owned(&users_order.order))
        } else {
            Err(ApiError::Unauthorized)
        }
    }
}