//! Shared job ownership and bounded, line-oriented child output.

use std::io::{self, BufRead, BufReader, ErrorKind, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::run::{OutputStream, RunJob, RunOutput};
use crate::plan_server::PlanMessage;

pub(crate) const CHILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_LINE_BYTES: usize = 4_096;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
type OutputLine = io::Result<(OutputStream, String)>;

pub(crate) fn run_streamed(
    cmd: Command,
    on_line: impl FnMut(OutputStream, &str),
) -> io::Result<ExitStatus> {
    run_streamed_bounded(cmd, CHILD_TIMEOUT, on_line)
}

fn run_streamed_bounded(
    mut cmd: Command,
    timeout: Duration,
    on_line: impl FnMut(OutputStream, &str),
) -> io::Result<ExitStatus> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    let stdout = child.stdout.take().expect("stdout configured as piped");
    let stderr = child.stderr.take().expect("stderr configured as piped");
    let (sender, receiver) = mpsc::sync_channel(128);
    let readers = [
        spawn_reader(stdout, OutputStream::Stdout, sender.clone()),
        spawn_reader(stderr, OutputStream::Stderr, sender),
    ];
    let result = collect_output(&mut child, &receiver, started, timeout, on_line);
    if result.is_err() {
        kill_child(&mut child);
    }
    drop(receiver);
    // On error, do not wait for descriptors inherited by descendants outside the group.
    // Dropping the receiver releases readers blocked on channel backpressure.
    if result.is_ok() {
        for reader in readers {
            reader
                .join()
                .map_err(|_| io::Error::other("child output reader panicked"))?;
        }
    }
    result
}

fn spawn_reader(
    pipe: impl Read + Send + 'static,
    stream: OutputStream,
    sender: mpsc::SyncSender<OutputLine>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let output = match read_capped_line(&mut reader) {
                Ok(Some(line)) => Ok((stream.clone(), line)),
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    break;
                }
            };
            if sender.send(output).is_err() {
                break;
            }
        }
    })
}

fn read_capped_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    // Keep enough extra bytes to decode a UTF-8 character straddling the limit.
    let mut prefix = Vec::with_capacity(MAX_LINE_BYTES + 4);
    let mut seen_bytes = false;
    loop {
        let available = match reader.fill_buf() {
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            result => result?,
        };
        if available.is_empty() {
            if !seen_bytes {
                return Ok(None);
            }
            break;
        }
        seen_bytes = true;
        let newline = available.iter().position(|&byte| byte == b'\n');
        let length = newline.unwrap_or(available.len());
        let keep = length.min((MAX_LINE_BYTES + 4) - prefix.len());
        prefix.extend_from_slice(&available[..keep]);
        reader.consume(length + usize::from(newline.is_some()));
        if newline.is_some() {
            if prefix.last() == Some(&b'\r') {
                prefix.pop();
            }
            break;
        }
    }
    let mut line = String::from_utf8_lossy(&prefix).into_owned();
    let mut end = line.len().min(MAX_LINE_BYTES);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line.truncate(end);
    Ok(Some(line))
}

fn collect_output(
    child: &mut Child,
    receiver: &mpsc::Receiver<OutputLine>,
    started: Instant,
    timeout: Duration,
    mut on_line: impl FnMut(OutputStream, &str),
) -> io::Result<ExitStatus> {
    let mut pipes_open = true;
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .ok_or_else(|| io::Error::new(ErrorKind::TimedOut, "child exceeded 30 minutes"))?;
        let pause = remaining.min(POLL_INTERVAL);
        if pipes_open {
            match receiver.recv_timeout(pause) {
                Ok(output) => {
                    let (stream, line) = output?;
                    on_line(stream, &line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => pipes_open = false,
            }
        } else if let Some(status) = child.try_wait()? {
            return Ok(status);
        } else {
            thread::sleep(pause);
        }
    }
}

fn kill_child(child: &mut Child) {
    // Descendants can retain the output pipes, so terminate the whole child group.
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Default, Debug)]
pub(crate) struct RunJobs {
    current: Mutex<Option<RunJob>>,
}

impl RunJobs {
    pub(crate) fn begin(self: &Arc<Self>, job: RunJob) -> Option<JobGuard> {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        if current.is_some() {
            return None;
        }
        *current = Some(job);
        Some(JobGuard {
            jobs: Arc::clone(self),
        })
    }

    pub(crate) fn current(&self) -> Option<RunJob> {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[derive(Debug)]
pub(crate) struct JobGuard {
    jobs: Arc<RunJobs>,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        *self
            .jobs
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

pub(crate) fn install_jobs(ctx: &PlanContext) {
    // Extensions has separate get/insert operations; serialize initialization only.
    static INSTALL: Mutex<()> = Mutex::new(());
    let _install = INSTALL.lock().unwrap_or_else(PoisonError::into_inner);
    if ctx.extensions().get::<RunJobs>().is_none() {
        ctx.extensions().insert(Arc::new(RunJobs::default()));
    }
}

pub(crate) fn jobs(ctx: &PlanContext) -> Arc<RunJobs> {
    ctx.extensions()
        .get::<RunJobs>()
        .expect("run jobs installed before use")
}

pub(crate) fn emit(ctx: &PlanContext, out: RunOutput) {
    ctx.hub().broadcast(&PlanMessage::RunOutput(out));
}

pub(crate) fn emit_line(
    ctx: &PlanContext,
    job: RunJob,
    step: &str,
    stream: OutputStream,
    line: &str,
) {
    emit(
        ctx,
        RunOutput {
            job,
            step: step.to_owned(),
            stream: Some(stream),
            line: Some(line.to_owned()),
            exit_code: None,
            done: false,
        },
    );
}

pub(crate) fn emit_done(ctx: &PlanContext, job: RunJob, step: &str, exit_code: i32) {
    emit(
        ctx,
        RunOutput {
            job,
            step: step.to_owned(),
            stream: None,
            line: None,
            exit_code: Some(exit_code),
            done: true,
        },
    );
}

#[cfg(test)]
mod tests;
