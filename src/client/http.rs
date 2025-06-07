use crate::client::Client;
use crate::client::client::BASE_URL;
use crate::error::ApiError;
use reqwest::Method as HttpMethod;
use serde::Serialize;

pub enum Method {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

// Honestly idk why i didn't just use reqwest::Method directly, but here we are
fn transform_method(method: Method) -> HttpMethod {
    match method {
        Method::Get => HttpMethod::GET,
        Method::Post => HttpMethod::POST,
        Method::Patch => HttpMethod::PATCH,
        Method::Put => HttpMethod::PUT,
        Method::Delete => HttpMethod::DELETE,
    }
}

impl<State> Client<State> {
    /**
    INTERNAL: Makes a request to the API, returning the response as a deserialized type.


    # Arguments
    - `method`: The HTTP method to use (GET, POST, PUT, DELETE).
    - `path`: The path to the API endpoint. (e.g., "/me").
    - `body`: An optional body to send with the request, serializable as JSON.

    # Returns
    - A `Result` containing the deserialized response or an `ApiError` on failure.
    */
    pub(crate) async fn call_api<T: serde::de::DeserializeOwned>(
        &mut self,
        method: Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T, ApiError> {
        let builder = self
            .http
            .request(transform_method(method), BASE_URL.to_owned() + path);

        let builder = if let Some(body) = body {
            builder.json(body)
        } else {
            builder
        };

        match builder.send().await {
            Ok(resp) => {
                let body = resp
                    .text()
                    .await
                    .map_err(|_| ApiError::Unknown("Error".to_string()))?;
                let data = serde_json::from_str::<T>(&body);
                
                match data {
                    Ok(data) => Ok(data),
                    Err(err) => { 
                        Err(ApiError::ParsingError(format!("Error Parsing: {:?}", err).to_string()))
                    },
                }
            }
            Err(_) => Err(ApiError::RequestError),
        }
    }
}
