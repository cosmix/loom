//! Parses the raw `LOOM_SESSION_TYPE` string into a [`SessionType`].
//!
//! `SessionType`'s `Display` impl (`models/session/types.rs`) is the wire
//! spelling the session wrapper script exports as `LOOM_SESSION_TYPE`, e.g.
//! `base_conflict` with an underscore. That is not the same spelling
//! `SessionType`'s serde attribute would produce (`rename_all = "lowercase"`
//! only lowercases, so `BaseConflict` would serialize as `baseconflict`), so
//! this mirrors `Display` exactly rather than going through serde.

use crate::models::session::SessionType;
use anyhow::{bail, Result};

pub(super) fn parse(raw: &str) -> Result<SessionType> {
    match raw {
        "stage" => Ok(SessionType::Stage),
        "merge" => Ok(SessionType::Merge),
        "base_conflict" => Ok(SessionType::BaseConflict),
        "knowledge" => Ok(SessionType::Knowledge),
        "adjudication" => Ok(SessionType::Adjudication),
        other => bail!("LOOM_SESSION_TYPE '{other}' is not a known session type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_display_spelling_back_to_its_session_type() {
        for session_type in [
            SessionType::Stage,
            SessionType::Merge,
            SessionType::BaseConflict,
            SessionType::Knowledge,
            SessionType::Adjudication,
        ] {
            assert_eq!(parse(&session_type.to_string()).unwrap(), session_type);
        }
    }

    #[test]
    fn rejects_an_unknown_session_type() {
        assert!(parse("bogus").is_err());
    }

    #[test]
    fn rejects_an_empty_session_type() {
        assert!(parse("").is_err());
    }
}
