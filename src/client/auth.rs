use serde_json::json;

use super::*;
use crate::client::ws::WsClientBuilder;
use crate::error::AuthError;
use crate::types::http::APIV1Result;
use crate::types::request::OrderCreationRequest;
use crate::types::request::OrderUpdateParams;
use crate::types::transaction::Transaction;
use std::collections::HashMap;

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
            rivens_cache: Vec::new(),
            token: None,
            device_id: None,
            limiter: build_limiter(REQUESTS_PER_SECOND).into(),
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
                            rivens_cache: self.rivens_cache,
                            token: Some(jwt.to_string()),
                            device_id: Some(device_id.parse().unwrap()),
                            limiter: build_limiter(REQUESTS_PER_SECOND).into(),
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

    fn build_auth_payload<'a>(
        &self,
        username: &'a str,
        password: &'a str,
        device_id: &'a str,
    ) -> HashMap<&'a str, &'a str> {
        let mut map = HashMap::new();
        map.insert("auth_type", "header");
        map.insert("email", username);
        map.insert("password", password);
        map.insert("device_id", device_id);
        map
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
    pub async fn my_orders(&self) -> Result<Vec<Order<Owned>>, ApiError> {
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
    pub fn take_order(&self, order: Order<Unowned>) -> Result<Order<Owned>, ApiError> {
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
    pub fn get_token(&self) -> String {
        // Only accessible on authed clients, if this panics we got hit by a cosmic particle
        self.token.clone().unwrap()
    }

    /**
    Returns the clients device id

    # Returns
    The Device ID used when authenticating
    */
    pub fn get_device_id(&self) -> String {
        // Again, panics, cosmic particle, you get the gist of it now
        self.device_id.clone().unwrap()
    }

    /**
    Create a WebSocket builder

    # Returns
    A WsClient Builder
    */
    pub fn create_websocket(&self) -> WsClientBuilder {
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
        &self,
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

    /**
     * Create a new order
     * # Arguments
     * - `args`: The [`OrderCreationRequest`][crate::types::request::OrderCreationRequest] to create the order with
     * # Returns
     * The created order
     */
    pub async fn create_order(&self, args: OrderCreationRequest) -> Result<Order<Owned>, ApiError> {
        let order: Result<ApiResult<OrderItem>, ApiError> =
            self.call_api(Method::Post, "/order", Some(&args)).await;

        Ok(Order::new_owned(&order?.data))
    }
    /**
    Close a portion or all of an existing order.
    Allows you to close part of an open order by specifying a quantity to reduce.
    For example, if your order was initially created with a quantity of 20, and you send a request to close 8 units, the remaining quantity will be 12.
    If you close the entire remaining quantity, the order will be considered fully closed and removed.
    # Arguments
    - `order_id`: The ID of the order to delete
    - `quantity`: The quantity of the order to delete
    # Returns
    - `Ok(Transaction)` if the order was successfully deleted
    - `Err(ApiError)` if there was an error deleting the order
    */

    pub async fn close_order(
        &self,
        order_id: &str,
        quantity: u32,
    ) -> Result<Transaction, ApiError> {
        let transaction: Result<ApiResult<Transaction>, ApiError> = self
            .call_api(
                Method::Post,
                format!("/order/{}/close", order_id).as_str(),
                Some(&json!({
                    "quantity": quantity
                })),
            )
            .await;

        Ok(transaction?.data)
    }

    /**
     * Delete an order
     * # Arguments
     * - `order_id`: The ID of the order to delete
     * # Returns
     * - `Ok(Order)` if the order was successfully deleted
     * - `Err(ApiError)` if there was an error deleting the order
     */
    pub async fn delete_order(&self, order_id: &str) -> Result<Order, ApiError> {
        let order: Result<ApiResult<OrderItem>, ApiError> = self
            .call_api(
                Method::Delete,
                format!("/order/{}", order_id).as_str(),
                None::<&NoBody>,
            )
            .await;

        Ok(Order::new(&order?.data))
    }
}
