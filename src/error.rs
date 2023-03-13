use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};

use tokio::sync::mpsc::error::SendError;
use tokio::task::JoinError;
use warp::http::StatusCode;
use warp::reject::Reject;
use warp::{Rejection, Reply};

pub struct GHAError {
    details: String,
}

impl GHAError {
    pub fn from_string(msg: String) -> GHAError {
        GHAError { details: msg }
    }
}

#[derive(Debug)]
pub enum PinError {
    InvalidPinValue { pin: u32, val: u32 },
    InvalidPin(u32),
}

impl Reject for PinError {}

impl Reject for GHAError {}

impl Display for PinError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let msg = match self {
            PinError::InvalidPinValue { pin, val } => {
                format!("InvalidPinValue {}: {} must be 0 or 1", pin, val)
            }
            PinError::InvalidPin(pin) => {
                format!("InvalidPin: pin {} not found", pin)
            }
        };
        f.write_str(msg.as_str())
    }
}

/// Handle warp Rejection's
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;
    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = StatusCode::NOT_FOUND.to_string()
    } else if let Some(e) = err.find::<PinError>() {
        code = StatusCode::BAD_REQUEST;
        message = format!("{}: {}", StatusCode::BAD_REQUEST, e)
    } else {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = StatusCode::INTERNAL_SERVER_ERROR.to_string()
    }
    Ok(warp::reply::with_status(message, code))
}

impl Display for GHAError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Debug for GHAError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for GHAError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl From<rppal::gpio::Error> for GHAError {
    fn from(value: rppal::gpio::Error) -> Self {
        GHAError {
            details: value.to_string(),
        }
    }
}

impl From<std::io::Error> for GHAError {
    fn from(value: std::io::Error) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<serde_yaml::Error> for GHAError {
    fn from(value: serde_yaml::Error) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<serde_merge::error::Error> for GHAError {
    fn from(value: serde_merge::error::Error) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<prometheus::Error> for GHAError {
    fn from(value: prometheus::Error) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<Box<dyn Error>> for GHAError {
    fn from(value: Box<dyn Error>) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl<T> From<SendError<T>> for GHAError {
    fn from(value: SendError<T>) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<JoinError> for GHAError {
    fn from(value: JoinError) -> Self {
        GHAError::from_string(value.to_string())
    }
}

impl From<PinError> for GHAError {
    fn from(value: PinError) -> Self {
        GHAError::from_string(value.to_string())
    }
}
