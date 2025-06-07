use crate::client::ws::{Route};
use crate::error::WsError;

#[test]
fn test_route_parsing_with_parameter() {
    let route = Route::parse("@wfm|subscribe/newOrders").unwrap();
    assert_eq!(route.protocol, "@wfm");
    assert_eq!(route.path, "subscribe/newOrders");
}

#[test]
fn test_route_parsing_without_parameter() {
    let route = Route::parse("@wfm|subscribe/newOrders").unwrap();
    assert_eq!(route.protocol, "@wfm");
    assert_eq!(route.path, "subscribe/newOrders");
}

#[test]
fn test_route_to_string() {
    let route_with_param = Route {
        protocol: "@wfm".to_string(),
        path: "cmd/subscribe/newOrders".to_string(),
        parameter: None,
    };
    assert_eq!(route_with_param.to_string(), "@wfm|cmd/subscribe/newOrders");
}

#[test]
fn test_route_parsing_invalid_format() {
    let result = Route::parse("invalid_route_format");
    assert!(result.is_err());
    match result {
        Err(WsError::InvalidPath(_)) => (),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn test_route_to_string_without_parameter() {
    let route = Route {
        protocol: "@wfm".to_string(),
        path: "event/user/login".to_string(),
        parameter: None,
    };
    assert_eq!(route.to_string(), "@wfm|event/user/login");
}
