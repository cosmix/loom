//! [`run_bounded_tail_output`]: a bounded run whose stdout keeps its end.

use anyhow::Result;
use std::io::Read;
use std::process::{Command, Output};
use std::sync::mpsc;
use std::time::Duration;

use super::errors::{ProcessTimeoutError, OUTPUT_LIMIT};
use super::owned::Expiry;
use super::{run_bounded_with, BoundedOutput, ReaderHandle};

/// What a run does with stdout past [`OUTPUT_LIMIT`].
#[derive(Clone, Copy)]
pub(super) enum Stdout {
    /// Report the excess as `ProcessOutputLimitError`.
    Whole,
    /// Drop the oldest bytes and report nothing.
    Tail,
}

/// [`super::run_bounded_output`] for output whose end matters most: stdout past
/// the capture limit loses its start instead of failing the run. stderr stays
/// reported.
pub(crate) fn run_bounded_tail_output(
    command: &mut Command,
    timeout: Duration,
    operation: impl Into<String>,
) -> Result<Output> {
    let operation = operation.into();
    match run_bounded_with(command, timeout, Stdout::Tail, Expiry::Kill)? {
        BoundedOutput::Completed(output) => Ok(output),
        BoundedOutput::TimedOut => Err(ProcessTimeoutError::new(operation, timeout).into()),
    }
}

/// [`super::drain`] keeping the last [`OUTPUT_LIMIT`] bytes.
pub(super) fn drain_tail<R: Read + Send + 'static>(
    pipe: Option<R>,
) -> std::io::Result<ReaderHandle> {
    let Some(mut pipe) = pipe else {
        return Ok(None);
    };
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("loom-process-tail".to_string())
        .spawn(move || {
            let _ = tx.send(read_tail(&mut pipe, OUTPUT_LIMIT));
        })?;
    Ok(Some(rx))
}

/// The last `limit` bytes of `pipe`, buffering less than twice that plus one
/// read chunk: the buffer is trimmed only after a read reaches `2 * limit`.
fn read_tail(pipe: &mut impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = pipe.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() >= limit.saturating_mul(2) {
            kept.drain(..kept.len() - limit);
        }
    }
    if kept.len() > limit {
        kept.drain(..kept.len() - limit);
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_is_exact_at_every_size_around_the_limit() {
        for size in [0, 1, 9, 10, 11, 19, 20, 21, 25_000] {
            let input: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
            let expected = &input[input.len().saturating_sub(10)..];
            let tail = read_tail(&mut input.as_slice(), 10).unwrap();
            assert_eq!(tail, expected, "{size}");
        }
    }

    #[test]
    fn stdout_past_the_limit_keeps_its_end_without_an_error() {
        let script = format!("head -c {} /dev/zero; printf LAST", OUTPUT_LIMIT + 1);
        let mut command = Command::new("sh");
        command.args(["-c", &script]);

        let output =
            run_bounded_tail_output(&mut command, Duration::from_secs(30), "tail").unwrap();

        assert_eq!(output.stdout.len(), OUTPUT_LIMIT);
        assert!(output.stdout.ends_with(b"LAST"));
    }

    #[test]
    fn stderr_past_the_limit_is_still_reported() {
        let script = format!("head -c {} /dev/zero >&2", OUTPUT_LIMIT + 1);
        let mut command = Command::new("sh");
        command.args(["-c", &script]);

        let error =
            run_bounded_tail_output(&mut command, Duration::from_secs(30), "tail").unwrap_err();

        let overflow = error
            .downcast_ref::<crate::process::ProcessOutputLimitError>()
            .unwrap();
        assert_eq!(overflow.stream, "stderr");
    }
}
