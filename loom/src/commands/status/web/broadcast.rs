//! Snapshot production from the daemon subscription or the local work files.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::commands::status::ui::tui::daemon_client;
use crate::daemon::{Response, ResponseFrameReader};
use crate::fs::work_dir::WorkDir;
use crate::status::data::{collect_status_data, StatusData};
use crate::web_server::hub::Hub;

use super::feed::Feed;
use super::model::{collect_snapshot, SnapshotSource, WebSnapshot};

#[cfg(test)]
#[path = "broadcast_tests.rs"]
mod tests;

const RECONNECT_ATTEMPTS: usize = 3;
const FILE_POLL_COUNT: usize = 5;
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Messages a subscriber may fall behind by before it is disconnected.
///
/// The daemon lane collects every ~1s (`STATUS_BROADCAST_INTERVAL_MS` in
/// `daemon/server/broadcast.rs`) and the file lane every [`FILE_POLL_INTERVAL`].
/// Each collect emits up to three small messages (`state`, `tokens`, `tick`),
/// plus a `full` every 30s, so 32 is a client that has not drained roughly ten
/// seconds of traffic: wedged rather than merely busy, a suspended tab whose
/// TCP window has closed, say. The write timeout in `web_server/ws.rs` cuts a
/// wedged peer as well. Buffering more would pin every queued message for the
/// life of the process; disconnecting costs the client a reconnect, which it
/// already performs and which yields a fresh `full`.
pub(super) const SUBSCRIBER_QUEUE_DEPTH: usize = 32;

/// Fans the dashboard's `/ws` messages out to all connected WebSocket clients.
#[derive(Clone)]
pub struct Broadcaster {
    inner: Arc<Inner>,
}

/// Lock order: `feed`, then the hub's `subscribers`. Both
/// [`Broadcaster::publish`] and [`Broadcaster::subscribe`] hold `feed` across
/// their hub call, so a new subscriber's `full` primer and the messages
/// published after it can neither interleave nor leave a gap between them.
struct Inner {
    feed: Mutex<Feed>,
    hub: Hub,
    terminals: bool,
}

/// The result of interpreting one daemon response.
pub enum DaemonStep {
    Snapshot(Box<WebSnapshot>),
    Ignore,
    Degraded(String),
}

enum DaemonExit {
    Disconnected { received_frame: bool },
    StreamError { received_frame: bool },
    Degraded(String),
}

impl Broadcaster {
    /// Start the producer thread for `base`.
    pub fn spawn(base: PathBuf, running: Arc<AtomicBool>, terminals: bool) -> Self {
        let this = Self::with_terminals(terminals);
        let producer = this.clone();
        thread::spawn(move || run(producer, base, running, terminals));
        this
    }

    /// Register a client. Its queue starts with a `full` message for the
    /// latest snapshot, if there is one.
    pub fn subscribe(&self) -> mpsc::Receiver<Arc<String>> {
        let mut feed = self.feed();
        let primer = feed.full_frame();
        self.inner.hub.subscribe(primer)
    }

    /// Return the latest snapshot as the bare JSON `/api/status` serves.
    pub fn latest(&self) -> Option<Arc<String>> {
        self.feed().snapshot_json()
    }

    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::with_terminals(false)
    }

    fn with_terminals(terminals: bool) -> Self {
        Self {
            inner: Arc::new(Inner {
                feed: Mutex::new(Feed::default()),
                hub: Hub::new(SUBSCRIBER_QUEUE_DEPTH),
                terminals,
            }),
        }
    }

    pub(super) fn terminals(&self) -> bool {
        self.inner.terminals
    }

    fn feed(&self) -> MutexGuard<'_, Feed> {
        self.inner.feed.lock().expect("dashboard feed poisoned")
    }

    /// Send the messages that bring subscribers to `snapshot`.
    pub(super) fn publish(&self, snapshot: WebSnapshot) {
        let mut feed = self.feed();
        self.publish_locked(&mut feed, snapshot);
    }

    /// [`Self::publish`] for a caller already holding the feed lock. The hub
    /// publish stays under it so two producers cannot interleave their messages.
    fn publish_locked(&self, feed: &mut Feed, snapshot: WebSnapshot) {
        match feed.accept(snapshot, Instant::now()) {
            Ok(frames) => {
                for frame in frames {
                    self.inner.hub.publish(frame);
                }
            }
            Err(error) => tracing::warn!("dashboard could not build frames: {error:#}"),
        }
    }
}

/// Serialize a status update as the exact daemon frame body.
pub fn snapshot_frame(
    work_path: &Path,
    data: StatusData,
    source: SnapshotSource,
    terminals: bool,
) -> Result<String> {
    serde_json::to_string(&collect_snapshot(work_path, data, source, terminals))
        .context("serialize dashboard snapshot")
}

/// Classify a daemon response without opening a daemon socket.
pub fn classify_response(work_path: &Path, response: Response, terminals: bool) -> DaemonStep {
    match response {
        Response::StatusUpdate { data } => DaemonStep::Snapshot(Box::new(collect_snapshot(
            work_path,
            *data,
            SnapshotSource::Daemon,
            terminals,
        ))),
        Response::Error { message } => DaemonStep::Degraded(message),
        _ => DaemonStep::Ignore,
    }
}

/// Produce a fresh file-backed frame for an HTTP request with no cached frame.
pub(super) fn fresh_file_snapshot(base: &Path, terminals: bool) -> Result<String> {
    let work_dir = WorkDir::new(base)?;
    work_dir.load()?;
    let data = collect_status_data(&work_dir)?;
    snapshot_frame(work_dir.root(), data, SnapshotSource::Files, terminals)
}

fn run(this: Broadcaster, base: PathBuf, running: Arc<AtomicBool>, terminals: bool) {
    let Ok(work_dir) = WorkDir::new(&base).and_then(|work_dir| {
        work_dir.load()?;
        Ok(work_dir)
    }) else {
        tracing::warn!("dashboard could not load work directory");
        return;
    };
    let work_path = work_dir.root().to_path_buf();
    let mut failures = 0;
    while running.load(Ordering::SeqCst) {
        match daemon_session(&work_path) {
            Ok(stream) => match forward_daemon(&this, stream, &work_path, &running, terminals) {
                DaemonExit::Degraded(message) => {
                    poll_files(
                        &this,
                        &work_dir,
                        &work_path,
                        &running,
                        Some(&message),
                        terminals,
                    );
                    failures = 0;
                }
                DaemonExit::Disconnected { received_frame }
                | DaemonExit::StreamError { received_frame } => {
                    if received_frame {
                        failures = 0;
                    }
                    failures += 1;
                    sleep_while_running(&running, Duration::from_millis(500));
                    if failures >= RECONNECT_ATTEMPTS {
                        poll_files(&this, &work_dir, &work_path, &running, None, terminals);
                        failures = 0;
                    }
                }
            },
            Err(error) => {
                tracing::debug!("dashboard daemon subscription unavailable: {error}");
                poll_files(&this, &work_dir, &work_path, &running, None, terminals);
                failures = 0;
            }
        }
    }
}

fn daemon_session(work_path: &Path) -> Result<UnixStream> {
    let mut stream = daemon_client::connect(work_path)?;
    daemon_client::subscribe(&mut stream)?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(stream)
}

fn forward_daemon(
    broadcaster: &Broadcaster,
    mut stream: UnixStream,
    work_path: &Path,
    running: &AtomicBool,
    terminals: bool,
) -> DaemonExit {
    let mut received_frame = false;
    let mut reader = ResponseFrameReader::new(Duration::from_secs(30));
    while running.load(Ordering::SeqCst) {
        match reader.poll(&mut stream) {
            Ok(Some(response)) => match classify_response(work_path, response, terminals) {
                DaemonStep::Snapshot(snapshot) => {
                    broadcaster.publish(*snapshot);
                    received_frame = true;
                }
                DaemonStep::Ignore => {}
                DaemonStep::Degraded(message) => return DaemonExit::Degraded(message),
            },
            Ok(None) => {}
            Err(error) if daemon_client::is_socket_disconnected(&error) => {
                return DaemonExit::Disconnected { received_frame };
            }
            Err(error) => {
                tracing::warn!("dashboard daemon stream error: {error}");
                return DaemonExit::StreamError { received_frame };
            }
        }
    }
    DaemonExit::Disconnected { received_frame }
}

fn poll_files(
    broadcaster: &Broadcaster,
    work_dir: &WorkDir,
    work_path: &Path,
    running: &AtomicBool,
    notice: Option<&str>,
    terminals: bool,
) {
    for _ in 0..FILE_POLL_COUNT {
        if !running.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = poll_files_once(broadcaster, work_dir, work_path, notice, terminals) {
            tracing::warn!("dashboard file snapshot failed: {error}");
        }
        sleep_while_running(running, FILE_POLL_INTERVAL);
    }
}

pub(super) fn poll_files_once(
    broadcaster: &Broadcaster,
    work_dir: &WorkDir,
    work_path: &Path,
    notice: Option<&str>,
    terminals: bool,
) -> Result<()> {
    let data = match collect_status_data(work_dir) {
        Ok(data) => data,
        Err(error) => {
            // Collection fails for the same reason the daemon lane degraded, so
            // returning here would leave the browser frozen with no explanation.
            let reason = notice.map_or_else(|| format!("{error:#}"), str::to_owned);
            if republish_with_notice(broadcaster, &reason) {
                return Ok(());
            }
            return Err(error);
        }
    };
    let mut snapshot = collect_snapshot(work_path, data, SnapshotSource::Files, terminals);
    if let Some(notice) = notice {
        snapshot.notice = Some(notice.to_owned());
    }
    broadcaster.publish(snapshot);
    Ok(())
}

/// Publish the latest snapshot again with `notice` set, so the browser shows
/// the failure above the rows it last received. Returns false when there is no
/// snapshot to carry the notice (nothing was published yet).
pub(super) fn republish_with_notice(broadcaster: &Broadcaster, notice: &str) -> bool {
    let mut feed = broadcaster.feed();
    let Some(mut snapshot) = feed.latest().cloned() else {
        return false;
    };
    snapshot.notice = Some(notice.to_owned());
    snapshot.generated_at = Utc::now();
    broadcaster.publish_locked(&mut feed, snapshot);
    true
}

fn sleep_while_running(running: &AtomicBool, duration: Duration) {
    let mut remaining = duration;
    while !remaining.is_zero() && running.load(Ordering::SeqCst) {
        let slice = remaining.min(Duration::from_millis(100));
        thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
}
