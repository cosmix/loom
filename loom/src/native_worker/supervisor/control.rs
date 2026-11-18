//! Poll control files for new cancellation requests and messages.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::super::model::{CancelRequest, ControlMessage};
use super::super::store::{self, StoreError};

#[derive(Debug)]
pub enum ControlEvent {
    Cancel(CancelRequest),
    Message(ControlMessage),
}

pub struct ControlPoller {
    dir: PathBuf,
    cancel_reported: bool,
    reported: HashSet<String>,
    last_error: Option<String>,
}

impl ControlPoller {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            cancel_reported: false,
            reported: HashSet::new(),
            last_error: None,
        }
    }

    /// Events that appeared since the previous call, plus the first store error not already reported.
    pub fn poll(&mut self) -> (Vec<ControlEvent>, Option<StoreError>) {
        let mut events = Vec::new();
        let mut error = None;
        let mut had_error = false;
        if !self.cancel_reported {
            match store::read_cancel(&self.dir) {
                Ok(Some(cancel)) => {
                    self.cancel_reported = true;
                    events.push(ControlEvent::Cancel(cancel));
                }
                Ok(None) => {}
                Err(err) => self.record_error(err, &mut error, &mut had_error),
            }
        }
        match store::read_messages(&self.dir) {
            Ok(messages) => {
                for message in messages {
                    if self.reported.insert(message.message_id.clone()) {
                        events.push(ControlEvent::Message(message));
                    }
                }
            }
            Err(err) => self.record_error(err, &mut error, &mut had_error),
        }
        if !had_error {
            self.last_error = None;
        }
        (events, error)
    }

    fn record_error(
        &mut self,
        err: StoreError,
        error: &mut Option<StoreError>,
        had_error: &mut bool,
    ) {
        if matches!(&err, StoreError::Io(io) if io.kind() == ErrorKind::NotFound) {
            return;
        }
        *had_error = true;
        let description = err.to_string();
        if error.is_none() && self.last_error.as_ref() != Some(&description) {
            self.last_error = Some(description);
            *error = Some(err);
        }
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
