//! Why the bridge refused a call.

use std::fmt;

use crate::plan_session::input::InputRefusal;

/// Why the bridge refused a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    NoSession,
    NotRunning,
    Busy(&'static str),
    Input(InputRefusal),
    Refused(String),
    Io(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSession => write!(f, "no such session"),
            Self::NotRunning => write!(f, "the chat is closing"),
            Self::Busy(why) => write!(f, "{why}"),
            Self::Input(refusal) => write!(f, "{refusal}"),
            Self::Refused(why) | Self::Io(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for BridgeError {}
