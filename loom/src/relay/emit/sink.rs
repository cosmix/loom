//! Where a relay call writes its stdout relay line and stderr status text.
//!
//! Production callers use [`StdSink`], which wraps the process's real
//! stdout/stderr; tests inject an in-memory sink so they can inspect exactly
//! what a call wrote without touching either stream.

use std::io::{self, Write};

/// The two streams one relay call writes: the human status text (stderr) and
/// the machine-readable `LOOM_RELAY_V1` line (stdout).
pub trait RelaySink {
    fn stdout(&mut self) -> &mut dyn Write;
    fn stderr(&mut self) -> &mut dyn Write;
}

/// The production [`RelaySink`]: the process's real stdout and stderr.
pub struct StdSink {
    stdout: io::Stdout,
    stderr: io::Stderr,
}

impl Default for StdSink {
    fn default() -> Self {
        Self {
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }
}

impl RelaySink for StdSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }

    fn stderr(&mut self) -> &mut dyn Write {
        &mut self.stderr
    }
}
