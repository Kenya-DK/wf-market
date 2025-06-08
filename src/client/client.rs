use crate::client::http::Method;
use crate::client::item::{Item, Regular};
use crate::client::order::{Order, Owned, Unowned};
use crate::client::utils::{AuthResp, build_http};
use crate::client::ws::WsClientBuilder;
use crate::error::{ApiError, AuthError};
use crate::types::filter::OrdersTopFilters;
use crate::types::http::{APIV1Result, ApiResult};
use crate::types::item::{Item as ItemObject, Order as OrderItem, OrderWithUser, OrdersTopResult};
use crate::types::request::OrderUpdateParams;
use crate::types::user::{FullUser, StatusType};
use serde::Serialize;
use std::collections::HashMap;
use std::marker::PhantomData;

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
    /// Internal item cache
    items_cache: Vec<Item>,

    token: Option<String>,
    device_id: Option<String>,

    _state: PhantomData<State>,
}

pub(super) const BASE_URL: &str = "https://api.warframe.market/v2";
pub(super) const V1_API: &str = "https://api.warframe.market/v1";

#[derive(Serialize)]
struct NoBody;

// Generic implementations (can be used with or without auth)
impl<State> Client<State> {
    /**
    Fetch all listed items from the WFM API

    # Returns
    List of all listed items
    */
    pub async fn get_items(&mut self) -> Result<Vec<Item<Regular>>, ApiError> {
        if !self.items_cache.is_empty() {
            let mut new_items = Vec::new();
            new_items.clone_from(&self.items_cache);
            return Ok(new_items);
        }

        let items: Result<ApiResult<Vec<ItemObject>>, ApiError> =
            self.call_api(Method::Get, "/items", None::<&NoBody>).await;

        Ok(items?.data.iter().map(|item| Item::new(item)).collect())
    }

    /**
    Fetch an item by an identifiable slug

    # Returns
    Full item object (currently the same from `get_items()`)
    */
    pub async fn get_item(&mut self, slug: &str) -> Result<Item<Regular>, ApiError> {
        let items: Result<ApiResult<ItemObject>, ApiError> = self
            .call_api(
                Method::Get,
                format!("/item/{}", slug).as_str(),
                None::<&NoBody>,
            )
            .await;

        Ok(Item::new(&items?.data))
    }

    /**
    Fetch all orders from users online within the last 7 days

    # Arguments
    - `slug`: The item whose orders you want to fetch

    # Returns
    A list of orders
    */
    pub async fn get_orders(&mut self, slug: &str) -> Result<Vec<Order<Unowned>>, ApiError> {
        let items: Result<ApiResult<Vec<OrderWithUser>>, ApiError> = self
            .call_api(
                Method::Get,
                format!("/orders/item/{}", slug).as_str(),
                None::<&NoBody>,
            )
            .await;

        Ok(items?
            .data
            .iter()
            .map(|order| Order::new(&order.downgrade()))
            .collect())
    }

    /**
    Fetch the top 5 orders for the specified slug

    # Arguments
    - `slug`: The item whose orders you want to fetch

    # Returns
    Total of 10 orders, top 5 buy/sell orders
    */
    pub async fn get_orders_top(
        &mut self,
        slug: &str,
        filters: Option<OrdersTopFilters>,
    ) -> Result<Vec<Order<Unowned>>, ApiError> {
        let query: String = if let Some(filters) = filters.clone() {
            let params = serde_urlencoded::to_string(filters)
                .map_err(|_| ApiError::ParsingError("Unable to serialize filters".to_string()))?;
            format!("?{}", params)
        } else {
            String::new()
        };

        let items: Result<ApiResult<OrdersTopResult>, ApiError> = self
            .call_api(
                Method::Get,
                format!("/orders/item/{}/top{}", slug, query).as_str(),
                None::<&NoBody>,
            )
            .await;

        let data = items?.data;
        
        let is_filtering_status = if let Some(filters) = filters.clone() {
            filters.user_activity.is_some()
        } else { false };

        let buy: Vec<Order<Unowned>> = data
            .buy
            .iter()
            .filter(|o| is_filtering_status && o.user.status_type == filters.clone().unwrap().user_activity.unwrap())
            .map(|order| Order::new(&order.downgrade()))
            .collect();
        let sell: Vec<Order<Unowned>> = data
            .sell
            .iter()
            .filter(|o| is_filtering_status && o.user.status_type == filters.clone().unwrap().user_activity.unwrap())
            .map(|order| Order::new(&order.downgrade()))
            .collect();

        let total: Vec<Order<Unowned>> = [buy, sell].concat();

        Ok(total)
    }

    /**
    Get the Item Type of an Order, fetches from updated list of items

    # Arguments
    - `order`: The order to get the type of

    # Returns
    A managed [`Item`][crate::client::item::Item] object
    */
    pub async fn get_order_item(&mut self, order: &Order) -> Result<Item<Regular>, ApiError> {
        if let Some(item) = self
            .get_items()
            .await?
            .iter()
            .find(|i| i.get_type().id == order.object.item_id)
        {
            return Ok(item.clone());
        }

        Err(ApiError::Unknown("Item not found".to_string()))
    }

    /**
    Get the order from an id

    # Arguments
    - `id`: An order ID

    # Returns
    A managed [`Order`][crate::client::order::Order] object
    */
    pub async fn get_order(&mut self, id: &str) -> Result<Order<Unowned>, ApiError> {
        let order: Result<ApiResult<OrderItem>, ApiError> = self
            .call_api(
                Method::Get,
                format!("/order/{}", id).as_str(),
                None::<&NoBody>,
            )
            .await;

        Ok(Order::new(&order?.data))
    }
}

impl Client<Unauthenticated> {
    /**
    Constructs a new client

    # Returns
    A client? duh?
    */
    pub fn new() -> Self {
        Client {
            http: build_http(None),
            user: None,
            orders: Vec::new(),
            status: StatusType::Offline,
            items_cache: Vec::new(),
            token: None,
            device_id: None,
            _state: PhantomData,
        }
    }

    /**
    Log in using username and password

    # Arguments
    - `username`: Users account username
    - `password`: Users account password
    - `device_id`: Unique identifier across the device, should not change between instances

    # Returns
    An authenticated client
    */
    pub async fn login(
        self,
        username: &str,
        password: &str,
        device_id: &str,
    ) -> Result<Client<Authenticated>, AuthError> {
        let mut map = HashMap::new();
        map.insert("auth_type", "header");
        map.insert("email", username);
        map.insert("password", password);
        map.insert("device_id", device_id);

        match self
            .http
            .post(V1_API.to_owned() + "/auth/signin")
            .json(&map)
            .header("Authorization", "JWT")
            .send()
            .await
        {
            Ok(resp) => {
                let headers = resp.headers().clone();
                let body = resp.text().await.unwrap();

                let data: APIV1Result<AuthResp> =
                    serde_json::from_str(&body).map_err(|_| AuthError::ParsingError)?;

                match headers.get("Authorization") {
                    Some(header) => {
                        let token: String = header
                            .to_str()
                            .map_err(|_| AuthError::ParsingError)?
                            .to_string();

                        let jwt = &token[4..]; // Remove the "JWT " from the token.
                        let http = build_http(Some(format!("Bearer {}", jwt)));

                        let mut authed_client = Client {
                            http,
                            user: Some(data.payload.user.clone()),
                            orders: Vec::new(),
                            status: data.payload.user.status_type,
                            items_cache: self.items_cache,
                            token: Some(jwt.to_string()),
                            device_id: Some(device_id.parse().unwrap()),
                            _state: PhantomData,
                        };

                        authed_client.refresh().await.map_err(|_| {
                            AuthError::Unknown(
                                "Unable to refresh user after authentication".to_string(),
                            )
                        })?;

                        Ok(authed_client)
                    }
                    None => Err(AuthError::ParsingError),
                }
            }
            Err(e) => Err(AuthError::Unknown(format!("Unknown Error: {:?}", e))),
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
        let orders: Result<ApiResult<Vec<OrderItem>>, ApiError> = self
            .call_api(Method::Get, "/orders/my", None::<&NoBody>)
            .await;

        let order_instances = orders?
            .data
            .iter()
            .map(|order| Order::new_owned(order))
            .collect();
        let user_data = user?.data;

        self.orders = order_instances;
        self.user = Some(user_data.clone());

        Ok(user_data)
    }

    /**
    Get the authenticated users orders

    # Returns
    List of all users orders
    */
    pub async fn my_orders(&mut self) -> Result<Vec<Order<Owned>>, ApiError> {
        let items: Result<ApiResult<Vec<OrderItem>>, ApiError> = self
            .call_api(Method::Get, "/orders/my", None::<&NoBody>)
            .await;

        Ok(items?
            .data
            .iter()
            .map(|order| Order::new_owned(order))
            .collect())
    }

    /**
    Take ownership of an order, converts an `<Unowned>` order to an `<Owned>` one

    # Note
    This is using the stored information from the last `.refresh()`,
    without a WebSocket connection this may be out of date unless manually updated

    # Arguments
    - `order`: Managed Order object

    # Returns
    - An Owned order
    */
    pub fn take_order(&mut self, order: Order<Unowned>) -> Result<Order<Owned>, ApiError> {
        if self
            .orders
            .iter()
            .find(|_order| _order.object.id == order.object.id)
            .is_some()
        {
            Ok(Order::new_owned(&order.get_type()))
        } else {
            Err(ApiError::Unauthorized)
        }
    }

    /**
    Return the authentication token

    # Returns
    The users JWT token
    */
    pub fn get_token(&mut self) -> String {
        // Only accessible on authed clients, if this panics we got hit by a cosmic particle
        self.token.clone().unwrap()
    }

    /**
    Returns the clients device id

    # Returns
    The Device ID used when authenticating
    */
    pub fn get_device_id(&mut self) -> String {
        // Again, panics, cosmic particle, you get the gist of it now
        self.device_id.clone().unwrap()
    }

    /**
    Create a WebSocket builder

    # Returns
    A WsClient Builder
    */
    pub fn create_websocket(&mut self) -> WsClientBuilder {
        WsClientBuilder::new(self.get_token(), self.get_device_id())
    }

    /**
    Update order information

    # Arguments
    - `order`: The [`Order`][crate::client::order::Order] to update

    # Example
    ```rust
    use wf_market::{
        client::Client,
        utils::generate_device_id,
        types::request::OrderUpdateParams,
    };

    async fn main() {
        let mut client = {
            // device_id should be stored and reused
            Client::new()
                .login("username", "password", generate_device_id().as_str())
                .await.unwrap()
        };

        if let Ok(orders) = client.my_orders().await {
            for order in orders {
                client.update_order(order, OrderUpdateParams {
                    platinum: Some(1), // Make all our orders basically free!
                    ..Default::default()
                })
            }
        }
    }
    ```

    # Returns
    The updated order
    */
    pub async fn update_order(
        &mut self,
        order: Order<Owned>,
        args: OrderUpdateParams,
    ) -> Result<Order<Owned>, ApiError> {
        let order: Result<ApiResult<OrderItem>, ApiError> = self
            .call_api(
                Method::Patch,
                format!("/order/{}", order.object.id).as_str(),
                Some(&args),
            )
            .await;

        Ok(Order::new_owned(&order?.data))
    }
}
