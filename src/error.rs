#[derive(Debug, Eq, PartialEq)]
pub enum AuthError {
    NoUser,
    ParsingError,
    Unknown(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum ApiError {
    ParsingError(String),
    RequestError,
    Unauthorized,
    Unknown(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum WsError {
    ReservedPath(String),
    InvalidPath(String),
    AlreadyRegistered(String),
    InvalidMessageReceived(String),
    ConnectionError,
    InvalidMessage,
    SendError,
    NotConnected,
}
