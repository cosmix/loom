use super::*;
use std::ffi::CString;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A private FIFO through which a criterion's persistent leader stops itself.
pub(crate) struct GroupAnchorAddress {
    work_dir: PathBuf,
    rel: PathBuf,
    token: String,
    pgid: u32,
    start: u64,
    boot: String,
    _parent_life: Option<UnixStream>,
}

impl GroupAnchorAddress {
    pub(crate) fn is_live(&self) -> bool {
        crate::process::process_start_time(self.pgid) == Some(self.start)
            && crate::process::boot_id::os_boot_id().ok().as_deref() == Some(&self.boot)
    }

    pub(crate) fn signal(&self) -> Result<()> {
        ensure!(self.is_live(), "criterion group anchor identity changed");
        let root = open_root(&self.work_dir)?;
        let flags = libc::O_WRONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW;
        let deadline = Instant::now() + Duration::from_millis(250);
        let fd = loop {
            match safe_fs::open_safely(root.as_raw_fd(), &self.rel, flags, 0) {
                Ok(fd) => break fd,
                Err(error)
                    if is_no_reader(&error) && Instant::now() < deadline && self.is_live() =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        let mut file = File::from(fd);
        ensure!(
            file.metadata()?.file_type().is_fifo(),
            "group control is not a FIFO"
        );
        file.write_all(format!("{}\n", self.token).as_bytes())?;
        Ok(())
    }
}

/// Start a leader that outlives the criterion command. Commands join this
/// group, while retirement writes the nonce and the leader signals itself.
pub(crate) fn start_guarded_group(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
) -> Result<(Child, GroupAnchorAddress)> {
    let job = job_rel(stage_id, job_id)?;
    let token = crate::host_verify::new_job_id();
    let name = format!("guard-{token}");
    let rel = job.join(&name);
    let root = open_root(work_dir)?;
    create_control_fifo(&root, &job, &name)?;
    let fifo = job_dir(work_dir, stage_id, job_id)?.join(&name);
    let (mut child, parent_life) = spawn_anchor_shell(&fifo, &token)?;
    // Publish only after the guardian is listening. A crash before this point
    // cannot strand a criterion, because the criterion has not spawned yet.
    let ready = (|| -> Result<GroupAnchorAddress> {
        let address = identify_anchor(work_dir, rel, token, child.id(), parent_life)?;
        wait_for_anchor_ready(&mut child, &address)?;
        record_anchor(&root, &job, &name, &address)?;
        Ok(address)
    })();
    let address = match ready {
        Ok(address) => address,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    Ok((child, address))
}

fn create_control_fifo(root: &OwnedFd, job: &Path, name: &str) -> Result<()> {
    let dir = safe_fs::open_safely(root.as_raw_fd(), job, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let c_name = CString::new(name.as_bytes())?;
    // SAFETY: `dir` is the opened job directory and `c_name` is one component.
    let created = unsafe { libc::mkfifoat(dir.as_raw_fd(), c_name.as_ptr(), 0o600) };
    if created != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn spawn_anchor_shell(fifo: &Path, token: &str) -> Result<(Child, UnixStream)> {
    let (reader, writer) = UnixStream::pair()?;
    let read_fd = reader.as_raw_fd();
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg("( IFS= read -r _ <&5; kill -s KILL 0 ) & exec 5<&-; exec 3< \"$1\"; exec 4> \"$1\"; printf 'READY\\n'; while IFS= read -r line <&3; do [ \"$line\" = \"$2\" ] && kill -s KILL 0; done")
        .arg("loom-group-anchor")
        .arg(fifo)
        .arg(token)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: after fork, the closure only calls async-signal-safe dup2/fcntl
    // to expose the parent-lifetime pipe as descriptor 5 to the shell.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(read_fd, 5) < 0 || libc::fcntl(5, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command
        .spawn()
        .context("cannot start criterion group anchor")?;
    drop(reader);
    Ok((child, writer))
}

fn identify_anchor(
    work_dir: &Path,
    rel: PathBuf,
    token: String,
    pgid: u32,
    parent_life: UnixStream,
) -> Result<GroupAnchorAddress> {
    let start = crate::process::process_start_time(pgid)
        .context("cannot read criterion group anchor start identity")?;
    let boot = crate::process::boot_id::os_boot_id()?;
    Ok(GroupAnchorAddress {
        work_dir: work_dir.to_path_buf(),
        rel,
        token,
        pgid,
        start,
        boot,
        _parent_life: Some(parent_life),
    })
}

fn record_anchor(
    root: &OwnedFd,
    job: &Path,
    name: &str,
    address: &GroupAnchorAddress,
) -> Result<()> {
    append_locked(root, &job.join(GROUPS_FILE), |file, _| {
        let mut held = Vec::new();
        file.take(GROUPS_MAX_BYTES as u64).read_to_end(&mut held)?;
        ensure!(
            held.iter().filter(|byte| **byte == b'\n').count() < MAX_GROUPS,
            "too many recorded criterion groups"
        );
        Ok(Some(
            format!(
                "{} {} {} {name} {}\n",
                address.pgid, address.start, address.boot, address.token
            )
            .into_bytes(),
        ))
    })
}

fn wait_for_anchor_ready(child: &mut Child, address: &GroupAnchorAddress) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let writer = loop {
        let root = open_root(&address.work_dir)?;
        let flags = libc::O_WRONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW;
        match safe_fs::open_safely(root.as_raw_fd(), &address.rel, flags, 0) {
            Ok(fd) => break fd,
            Err(error) if Instant::now() < deadline && is_no_reader(&error) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(error).context("criterion group anchor did not open its control FIFO")
            }
        }
    };
    let stdout = child
        .stdout
        .as_mut()
        .context("criterion group anchor has no ready pipe")?;
    let mut poll = libc::pollfd {
        fd: stdout.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: `poll` points to one initialized pollfd whose descriptor is owned by child.
    let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
    ensure!(ready > 0, "criterion group anchor did not become ready");
    let mut marker = [0u8; 6];
    stdout.read_exact(&mut marker)?;
    ensure!(
        &marker == b"READY\n",
        "criterion group anchor sent invalid readiness"
    );
    drop(writer);
    Ok(())
}

fn is_no_reader(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<std::io::Error>()
        .and_then(std::io::Error::raw_os_error)
        == Some(libc::ENXIO)
}

/// Legacy group records remain readable but never authorize a raw PGID signal.
pub fn append_group(work_dir: &Path, stage_id: &str, job_id: &str, pgid: u32) -> Result<()> {
    let rel = job_rel(stage_id, job_id)?.join(GROUPS_FILE);
    let identity =
        crate::process::process_start_time(pgid).zip(crate::process::boot_id::os_boot_id().ok());
    append_locked(&open_root(work_dir)?, &rel, |file, _| {
        let mut held = Vec::new();
        file.take(GROUPS_MAX_BYTES as u64)
            .read_to_end(&mut held)
            .context("cannot read the job's process groups")?;
        let lines = held.iter().filter(|byte| **byte == b'\n').count();
        Ok((lines < MAX_GROUPS).then(|| match &identity {
            Some((start, boot)) => format!("{pgid} {start} {boot}\n").into_bytes(),
            None => format!("{pgid}\n").into_bytes(),
        }))
    })
}

pub fn load_groups(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<u32>> {
    let rel = job_rel(stage_id, job_id)?.join(GROUPS_FILE);
    let Some(bytes) = read_in(work_dir, &rel, GROUPS_MAX_BYTES)? else {
        return Ok(Vec::new());
    };
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.split_whitespace().next()?.parse::<u32>().ok())
        .filter(|pgid| *pgid > 1)
        .collect())
}

pub(crate) fn load_group_anchors(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
) -> Result<Vec<GroupAnchorAddress>> {
    let rel = job_rel(stage_id, job_id)?.join(GROUPS_FILE);
    let Some(bytes) = read_in(work_dir, &rel, GROUPS_MAX_BYTES)? else {
        return Ok(Vec::new());
    };
    let job = job_rel(stage_id, job_id)?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pgid = fields.next()?.parse::<u32>().ok()?;
            let start = fields.next()?.parse::<u64>().ok()?;
            let boot = fields.next()?;
            let name = fields.next()?;
            let token = fields.next()?;
            if pgid <= 1
                || fields.next().is_some()
                || name != format!("guard-{token}")
                || !token
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
            {
                return None;
            }
            Some(GroupAnchorAddress {
                work_dir: work_dir.to_path_buf(),
                rel: job.join(name),
                token: token.to_owned(),
                pgid,
                start,
                boot: boot.to_owned(),
                _parent_life: None,
            })
        })
        .collect())
}
