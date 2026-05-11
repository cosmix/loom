mod methods;
mod transitions;
mod types;

pub use types::{Session, SessionStatus, SessionType};

// Re-export BackendType so callers don't have to reach into plan::schema
// just to talk about the backend a session is running on.
pub use crate::plan::schema::execution::BackendType;

#[cfg(test)]
mod tests;
