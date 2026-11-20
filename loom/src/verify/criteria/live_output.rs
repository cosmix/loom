//! A criterion's output copied to a writer as it arrives, so that a run that
//! hangs shows how far it got before it ends.

use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

/// The function a [`LiveOutput`] wraps: the writer for the criterion at a
/// 0-based index, or `None` to copy nothing.
pub type LiveWriterOpener = dyn Fn(usize) -> Option<Box<dyn Write + Send>> + Send + Sync;

/// Opens, as each criterion starts to run, the writer its stdout and stderr
/// are copied to in arrival order. A result answered from the pass cache runs
/// nothing and opens no writer.
#[derive(Clone)]
pub struct LiveOutput(pub Arc<LiveWriterOpener>);

impl LiveOutput {
    pub(super) fn open(&self, index: usize) -> Option<LiveSink> {
        (self.0)(index).map(|writer| LiveSink(Arc::new(Mutex::new(Some(writer)))))
    }
}

impl fmt::Debug for LiveOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveOutput").finish_non_exhaustive()
    }
}

/// One criterion's writer, shared by its stdout and stderr readers. The first
/// failed write stops the copy; the captured output does not depend on it.
#[derive(Clone)]
pub(super) struct LiveSink(Arc<Mutex<Option<Box<dyn Write + Send>>>>);

impl LiveSink {
    pub(super) fn write(&self, bytes: &[u8]) {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(writer) = slot.as_mut() else {
            return;
        };
        if writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .is_err()
        {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailsAfter(Arc<Mutex<Vec<u8>>>, usize);

    impl Write for FailsAfter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.1 == 0 {
                return Err(std::io::Error::other("full"));
            }
            self.1 -= 1;
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failed_write_stops_the_copy() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let held = Arc::clone(&seen);
        let live = LiveOutput(Arc::new(move |index| {
            (index == 3)
                .then(|| Box::new(FailsAfter(Arc::clone(&held), 1)) as Box<dyn Write + Send>)
        }));
        assert!(live.open(0).is_none());
        let sink = live.open(3).unwrap();
        for chunk in [&b"one "[..], b"two ", b"three"] {
            sink.write(chunk);
        }
        assert_eq!(*seen.lock().unwrap(), b"one ");
        assert!(sink.0.lock().unwrap().is_none());
    }
}
