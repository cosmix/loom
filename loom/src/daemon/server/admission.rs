//! Resource admission and monotonic read deadlines for daemon clients.

use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Reader whose timeout always refers to one absolute deadline.
pub(super) struct DeadlineReader<'a> {
    stream: &'a UnixStream,
    deadline: Instant,
}

impl<'a> DeadlineReader<'a> {
    pub(super) fn new(stream: &'a UnixStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }

    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline elapsed"))
    }
}

impl Read for DeadlineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.remaining()?;
        self.stream.set_read_timeout(Some(remaining))?;
        let mut stream = self.stream;
        stream.read(buffer)
    }
}

/// Global cap on request-body bytes allocated by concurrent client workers.
pub(super) struct ByteBudget {
    limit: usize,
    in_flight: AtomicUsize,
}

impl ByteBudget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            in_flight: AtomicUsize::new(0),
        })
    }

    pub(super) fn try_reserve(self: &Arc<Self>, bytes: usize) -> Option<BytePermit> {
        if bytes > self.limit {
            return None;
        }
        let mut current = self.in_flight.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(bytes)?;
            if next > self.limit {
                return None;
            }
            match self.in_flight.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(BytePermit {
                        budget: Arc::clone(self),
                        bytes,
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

pub(super) struct BytePermit {
    budget: Arc<ByteBudget>,
    bytes: usize,
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        self.budget
            .in_flight
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::thread;

    #[test]
    fn budget_rejects_exhaustion_and_releases_on_drop() {
        let budget = ByteBudget::new(10);
        let first = budget.try_reserve(7).expect("first reservation");
        assert!(budget.try_reserve(4).is_none());
        let second = budget.try_reserve(3).expect("remaining capacity");
        assert!(budget.try_reserve(1).is_none());
        drop(first);
        assert!(budget.try_reserve(7).is_some());
        drop(second);
    }

    #[test]
    fn slow_drip_cannot_extend_absolute_deadline() {
        let (reader_stream, mut writer_stream) = UnixStream::pair().unwrap();
        let writer = thread::spawn(move || {
            for _ in 0..10 {
                thread::sleep(Duration::from_millis(35));
                if writer_stream.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let started = Instant::now();
        let mut reader = DeadlineReader::new(&reader_stream, Duration::from_millis(120));
        let mut bytes = [0u8; 10];

        let error = reader.read_exact(&mut bytes).unwrap_err();

        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
        assert!(started.elapsed() < Duration::from_millis(300));
        drop(reader_stream);
        writer.join().unwrap();
    }

    #[test]
    fn complete_input_before_deadline_is_read() {
        let (reader_stream, mut writer_stream) = UnixStream::pair().unwrap();
        writer_stream.write_all(b"valid").unwrap();
        let mut reader = DeadlineReader::new(&reader_stream, Duration::from_secs(1));
        let mut bytes = [0u8; 5];

        reader.read_exact(&mut bytes).unwrap();

        assert_eq!(&bytes, b"valid");
    }
}
