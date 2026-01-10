use super::protocol::{read_message, write_message, DaemonConfig, Request, Response, StageInfo};
use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::models::worktree::WorktreeStatus;
use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::parser::frontmatter::extract_yaml_frontmatter;
use crate::plan::graph::ExecutionGraph;
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;

/// Interval between status broadcasts in milliseconds.
const STATUS_BROADCAST_INTERVAL_MS: u64 = 1000;

/// Daemon server that listens on a Unix domain socket.
pub struct DaemonServer {
    socket_path: PathBuf,
    pid_path: PathBuf,
    log_path: PathBuf,
    work_dir: PathBuf,
    config: DaemonConfig,
    shutdown_flag: Arc<AtomicBool>,
    status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
}

impl DaemonServer {
    /// Create a new daemon server with default configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn new(work_dir: &Path) -> Self {
        Self::with_config(work_dir, DaemonConfig::default())
    }

    /// Create a new daemon server with custom configuration.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    /// * `config` - The daemon configuration
    ///
    /// # Returns
    /// A new `DaemonServer` instance
    pub fn with_config(work_dir: &Path, config: DaemonConfig) -> Self {
        Self {
            socket_path: work_dir.join("orchestrator.sock"),
            pid_path: work_dir.join("orchestrator.pid"),
            log_path: work_dir.join("orchestrator.log"),
            work_dir: work_dir.to_path_buf(),
            config,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            status_subscribers: Arc::new(Mutex::new(Vec::new())),
            log_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if a daemon is already running.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `true` if a daemon is running, `false` otherwise
    pub fn is_running(work_dir: &Path) -> bool {
        let pid_path = work_dir.join("orchestrator.pid");

        if let Some(pid) = Self::read_pid(work_dir) {
            // Check if process exists by sending signal 0
            let pid_exists = unsafe { libc::kill(pid as i32, 0) == 0 };
            if !pid_exists {
                // PID file exists but process is dead, clean up stale file
                let _ = fs::remove_file(pid_path);
                return false;
            }
            true
        } else {
            false
        }
    }

    /// Read the PID from the PID file.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `Some(pid)` if the file exists and contains a valid PID, `None` otherwise
    pub fn read_pid(work_dir: &Path) -> Option<u32> {
        let pid_path = work_dir.join("orchestrator.pid");
        fs::read_to_string(pid_path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    /// Stop a running daemon by sending a stop request via socket.
    ///
    /// # Arguments
    /// * `work_dir` - The .work/ directory path
    ///
    /// # Returns
    /// `Ok(())` on success, error if daemon is not running or communication fails
    pub fn stop(work_dir: &Path) -> Result<()> {
        let socket_path = work_dir.join("orchestrator.sock");

        if !Self::is_running(work_dir) {
            anyhow::bail!("Daemon is not running");
        }

        let mut stream =
            UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

        write_message(&mut stream, &Request::Stop).context("Failed to send stop request")?;

        let response: Response =
            read_message(&mut stream).context("Failed to read stop response")?;

        match response {
            Response::Ok => Ok(()),
            Response::Error { message } => anyhow::bail!("Daemon returned error: {message}"),
            _ => anyhow::bail!("Unexpected response from daemon"),
        }
    }

    /// Start the daemon (daemonize process).
    ///
    /// # Returns
    /// `Ok(())` on success, error if daemon fails to start
    pub fn start(&self) -> Result<()> {
        // Check if socket already exists
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path).context("Failed to remove stale socket file")?;
        }

        // Daemonize the process
        let daemonize = daemonize::Daemonize::new()
            .pid_file(&self.pid_path)
            .working_directory(".")
            .stdout(fs::File::create(&self.log_path).context("Failed to create log file")?)
            .stderr(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)
                    .context("Failed to open log file for stderr")?,
            );

        daemonize.start().context("Failed to daemonize process")?;

        // Run the server
        self.run_server()
    }

    /// Run the daemon in foreground (for testing).
    ///
    /// # Returns
    /// `Ok(())` on success, error if server fails to start
    pub fn run_foreground(&self) -> Result<()> {
        // Write PID file manually
        fs::write(&self.pid_path, format!("{}", std::process::id()))
            .context("Failed to write PID file")?;

        // Check if socket already exists
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path).context("Failed to remove stale socket file")?;
        }

        self.run_server()
    }

    /// Main server loop (listens on socket and accepts connections).
    fn run_server(&self) -> Result<()> {
        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;

        // Set socket to non-blocking mode for graceful shutdown
        listener
            .set_nonblocking(true)
            .context("Failed to set socket to non-blocking")?;

        // Spawn the orchestrator thread to actually run stages
        let orchestrator_handle = self.spawn_orchestrator();

        // Spawn log tailing thread
        let log_tail_handle = self.spawn_log_tailer();

        // Spawn status broadcasting thread
        let status_broadcast_handle = self.spawn_status_broadcaster();

        while !self.shutdown_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let shutdown_flag = Arc::clone(&self.shutdown_flag);
                    let status_subscribers = Arc::clone(&self.status_subscribers);
                    let log_subscribers = Arc::clone(&self.log_subscribers);

                    thread::spawn(move || {
                        if let Err(e) = Self::handle_client_connection(
                            stream,
                            shutdown_flag,
                            status_subscribers,
                            log_subscribers,
                        ) {
                            eprintln!("Client handler error: {e}");
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection available, sleep briefly
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("Accept error: {e}");
                    break;
                }
            }
        }

        // Wait for threads to finish
        if let Some(handle) = orchestrator_handle {
            let _ = handle.join();
        }
        if let Some(handle) = log_tail_handle {
            let _ = handle.join();
        }
        if let Some(handle) = status_broadcast_handle {
            let _ = handle.join();
        }

        self.cleanup()?;
        Ok(())
    }

    /// Spawn the orchestrator thread to execute stages.
    ///
    /// Returns a join handle for the orchestrator thread.
    fn spawn_orchestrator(&self) -> Option<JoinHandle<()>> {
        let work_dir = self.work_dir.clone();
        let daemon_config = self.config.clone();
        let shutdown_flag = Arc::clone(&self.shutdown_flag);

        Some(thread::spawn(move || {
            if let Err(e) = Self::run_orchestrator(&work_dir, &daemon_config, shutdown_flag) {
                eprintln!("Orchestrator error: {e}");
            }
        }))
    }

    /// Run the orchestrator loop (static method for thread).
    fn run_orchestrator(
        work_dir: &Path,
        daemon_config: &DaemonConfig,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Result<()> {
        // Build execution graph from stage files
        let graph = Self::build_execution_graph(work_dir)?;

        // Get repo root (parent of .work/)
        let repo_root = work_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // Configure orchestrator using daemon config
        let config = OrchestratorConfig {
            max_parallel_sessions: daemon_config.max_parallel.unwrap_or(4),
            poll_interval: Duration::from_secs(5),
            manual_mode: daemon_config.manual_mode,
            watch_mode: daemon_config.watch_mode,
            work_dir: work_dir.to_path_buf(),
            repo_root,
            status_update_interval: Duration::from_secs(30),
            backend_type: BackendType::Native,
            auto_merge: daemon_config.auto_merge,
        };

        // Create and run orchestrator
        let mut orchestrator =
            Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

        println!("Orchestrator started, spawning ready stages...");

        // Check shutdown flag before starting
        if shutdown_flag.load(Ordering::Relaxed) {
            println!("Orchestrator shutdown requested before start");
            return Ok(());
        }

        // Run orchestrator - it runs its own loop internally and returns when complete
        match orchestrator.run() {
            Ok(result) => {
                if !result.completed_stages.is_empty() {
                    println!("Completed stages: {}", result.completed_stages.join(", "));
                }
                if !result.failed_stages.is_empty() {
                    println!("Failed stages: {}", result.failed_stages.join(", "));
                }
                if result.is_success() {
                    println!("All stages completed successfully");
                }
            }
            Err(e) => {
                eprintln!("Orchestrator run error: {e}");
            }
        }

        Ok(())
    }

    /// Build execution graph from .work/stages/ files.
    fn build_execution_graph(work_dir: &Path) -> Result<ExecutionGraph> {
        let stages_dir = work_dir.join("stages");

        if stages_dir.exists() {
            let stages = Self::load_stages_from_work_dir(&stages_dir)?;
            if !stages.is_empty() {
                return ExecutionGraph::build(stages)
                    .context("Failed to build execution graph from stage files");
            }
        }

        // Fall back to reading from plan file
        let config_path = work_dir.join("config.toml");

        if !config_path.exists() {
            anyhow::bail!("No active plan. Run 'loom init <plan-path>' first.");
        }

        let config_content =
            fs::read_to_string(&config_path).context("Failed to read config.toml")?;

        let config: toml::Value =
            toml::from_str(&config_content).context("Failed to parse config.toml")?;

        let source_path = config
            .get("plan")
            .and_then(|p| p.get("source_path"))
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("No 'plan.source_path' found in config.toml"))?;

        let path = PathBuf::from(source_path);

        if !path.exists() {
            anyhow::bail!(
                "Plan file not found: {}\nThe plan may have been moved or deleted.",
                path.display()
            );
        }

        let parsed_plan = parse_plan(&path)
            .with_context(|| format!("Failed to parse plan: {}", path.display()))?;

        ExecutionGraph::build(parsed_plan.stages).context("Failed to build execution graph")
    }

    /// Load stage definitions from .work/stages/ directory.
    fn load_stages_from_work_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
        let mut stages = Vec::new();

        for entry in fs::read_dir(stages_dir)
            .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            // Skip non-markdown files
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Read and parse the stage file
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

            // Extract YAML frontmatter
            let frontmatter = match Self::extract_stage_frontmatter(&content) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                    continue;
                }
            };

            stages.push(frontmatter);
        }

        Ok(stages)
    }

    /// Extract stage definition from YAML frontmatter.
    fn extract_stage_frontmatter(content: &str) -> Result<StageDefinition> {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || !lines[0].trim().starts_with("---") {
            anyhow::bail!("No frontmatter delimiter found");
        }

        let mut end_idx = None;
        for (idx, line) in lines.iter().enumerate().skip(1) {
            if line.trim().starts_with("---") {
                end_idx = Some(idx);
                break;
            }
        }

        let end_idx = end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed"))?;

        let yaml_content = lines[1..end_idx].join("\n");

        #[derive(serde::Deserialize)]
        struct StageFrontmatter {
            id: String,
            name: String,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            dependencies: Vec<String>,
            #[serde(default)]
            parallel_group: Option<String>,
            #[serde(default)]
            acceptance: Vec<String>,
            #[serde(default)]
            setup: Vec<String>,
            #[serde(default)]
            files: Vec<String>,
        }

        let fm: StageFrontmatter = serde_yaml::from_str(&yaml_content)
            .context("Failed to parse stage YAML frontmatter")?;

        Ok(StageDefinition {
            id: fm.id,
            name: fm.name,
            description: fm.description,
            dependencies: fm.dependencies,
            parallel_group: fm.parallel_group,
            acceptance: fm.acceptance,
            setup: fm.setup,
            files: fm.files,
            auto_merge: None,
        })
    }

    /// Spawn the log tailing thread.
    ///
    /// Returns a join handle if the log file exists and the thread was spawned.
    fn spawn_log_tailer(&self) -> Option<JoinHandle<()>> {
        if !self.log_path.exists() {
            return None;
        }

        let log_path = self.log_path.clone();
        let shutdown_flag = Arc::clone(&self.shutdown_flag);
        let log_subscribers = Arc::clone(&self.log_subscribers);

        Some(thread::spawn(move || {
            if let Err(e) = Self::run_log_tailer(&log_path, shutdown_flag, log_subscribers) {
                eprintln!("Log tailer error: {e}");
            }
        }))
    }

    /// Run the log tailer loop (static method for thread).
    fn run_log_tailer(
        log_path: &Path,
        shutdown_flag: Arc<AtomicBool>,
        log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    ) -> Result<()> {
        let log_file = fs::File::open(log_path).context("Failed to open log file for tailing")?;
        let mut reader = BufReader::new(log_file);

        // Seek to end of file to only tail new content
        reader.seek(SeekFrom::End(0))?;

        let mut line = String::new();

        while !shutdown_flag.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // No new data, sleep briefly
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(_) => {
                    let response = Response::LogLine {
                        line: line.trim_end().to_string(),
                    };
                    if let Ok(mut subs) = log_subscribers.lock() {
                        subs.retain_mut(|stream| write_message(stream, &response).is_ok());
                    }
                }
                Err(e) => {
                    eprintln!("Error reading log file: {e}");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Spawn the status broadcasting thread.
    fn spawn_status_broadcaster(&self) -> Option<JoinHandle<()>> {
        let work_dir = self.work_dir.clone();
        let shutdown_flag = Arc::clone(&self.shutdown_flag);
        let status_subscribers = Arc::clone(&self.status_subscribers);

        Some(thread::spawn(move || {
            Self::run_status_broadcaster(&work_dir, shutdown_flag, status_subscribers);
        }))
    }

    /// Run the status broadcaster loop (static method for thread).
    fn run_status_broadcaster(
        work_dir: &Path,
        shutdown_flag: Arc<AtomicBool>,
        status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    ) {
        while !shutdown_flag.load(Ordering::Relaxed) {
            // Only broadcast if there are subscribers
            let has_subscribers = status_subscribers
                .lock()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            if has_subscribers {
                if let Ok(status_update) = Self::collect_status(work_dir) {
                    if let Ok(mut subs) = status_subscribers.lock() {
                        subs.retain_mut(|stream| write_message(stream, &status_update).is_ok());
                    }
                }
            }

            thread::sleep(Duration::from_millis(STATUS_BROADCAST_INTERVAL_MS));
        }
    }

    /// Collect current stage status from the work directory.
    fn collect_status(work_dir: &Path) -> Result<Response> {
        let stages_dir = work_dir.join("stages");
        let sessions_dir = work_dir.join("sessions");

        // Get repo root (parent of .work/)
        let repo_root = work_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut stages_executing = Vec::new();
        let mut stages_pending = Vec::new();
        let mut stages_completed = Vec::new();
        let mut stages_blocked = Vec::new();

        // Read stages directory
        if stages_dir.exists() {
            if let Ok(entries) = fs::read_dir(&stages_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Some((id, name, status, session_id)) =
                                Self::parse_stage_frontmatter(&content)
                            {
                                match status.as_str() {
                                    "executing" => {
                                        let session_pid = Self::get_session_pid(
                                            &sessions_dir,
                                            session_id.as_deref(),
                                        );
                                        let started_at = Self::get_stage_started_at(&content);
                                        let worktree_status =
                                            Self::detect_worktree_status(&id, &repo_root);
                                        stages_executing.push(StageInfo {
                                            id,
                                            name,
                                            session_pid,
                                            started_at,
                                            worktree_status,
                                        });
                                    }
                                    "waiting-for-deps" | "pending" | "queued" | "ready" => {
                                        stages_pending.push(id);
                                    }
                                    "completed" | "verified" => {
                                        stages_completed.push(id);
                                    }
                                    "blocked" | "needshandoff" | "waiting-for-input" => {
                                        stages_blocked.push(id);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Response::StatusUpdate {
            stages_executing,
            stages_pending,
            stages_completed,
            stages_blocked,
        })
    }

    /// Detect the worktree status for a stage.
    ///
    /// Returns the appropriate WorktreeStatus based on:
    /// - Whether the worktree directory exists
    /// - Whether there are merge conflicts
    /// - Whether a merge is in progress
    /// - Whether the branch was manually merged outside of loom
    fn detect_worktree_status(stage_id: &str, repo_root: &Path) -> Option<WorktreeStatus> {
        let worktree_path = repo_root.join(".worktrees").join(stage_id);

        if !worktree_path.exists() {
            return None;
        }

        // Check for merge conflicts using git diff --name-only --diff-filter=U
        if Self::has_merge_conflicts(&worktree_path) {
            return Some(WorktreeStatus::Conflict);
        }

        // Check if a merge is in progress by looking for MERGE_HEAD
        let merge_head = worktree_path.join(".git").join("MERGE_HEAD");
        // For worktrees, .git is a file pointing to the main repo, so check gitdir
        let git_path = worktree_path.join(".git");
        let is_merging = if git_path.is_file() {
            // Read gitdir path and check for MERGE_HEAD there
            if let Ok(content) = fs::read_to_string(&git_path) {
                if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                    let gitdir_path = PathBuf::from(gitdir.trim());
                    gitdir_path.join("MERGE_HEAD").exists()
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            merge_head.exists()
        };

        if is_merging {
            return Some(WorktreeStatus::Merging);
        }

        // Check if the branch was manually merged outside loom
        // This detects when users run `git merge loom/stage-id` manually
        if Self::is_manually_merged(stage_id, repo_root) {
            return Some(WorktreeStatus::Merged);
        }

        Some(WorktreeStatus::Active)
    }

    /// Check if a loom branch has been manually merged into the default branch.
    ///
    /// This is used to detect merges performed outside of loom (e.g., via CLI).
    /// When detected, the orchestrator can trigger cleanup of the worktree.
    fn is_manually_merged(stage_id: &str, repo_root: &Path) -> bool {
        use crate::git::{default_branch, is_branch_merged};

        // Get the default branch (main/master)
        let target_branch = match default_branch(repo_root) {
            Ok(branch) => branch,
            Err(_) => return false,
        };

        // Check if the loom branch has been merged into the target branch
        let branch_name = format!("loom/{stage_id}");
        is_branch_merged(&branch_name, &target_branch, repo_root).unwrap_or_default()
    }

    /// Check if there are unmerged paths (merge conflicts) in the worktree
    fn has_merge_conflicts(worktree_path: &Path) -> bool {
        let output = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=U"])
            .current_dir(worktree_path)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                !stdout.trim().is_empty()
            }
            Err(_) => false,
        }
    }

    /// Parse stage frontmatter to extract id, name, status, and session.
    ///
    /// Uses proper YAML parsing via serde_yaml for robustness. This handles
    /// all YAML formats correctly (quoted strings, flow style, multiline values, etc.)
    fn parse_stage_frontmatter(content: &str) -> Option<(String, String, String, Option<String>)> {
        // Use proper YAML parsing instead of line-by-line string matching
        let yaml = extract_yaml_frontmatter(content).ok()?;

        // Extract required fields
        let id = yaml
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())?;

        let name = yaml
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())?;

        let status = yaml
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())?;

        // Extract optional session field
        let session = yaml
            .get("session")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "null" && *s != "~")
            .map(|s| s.to_string());

        Some((id, name, status, session))
    }

    /// Get the started_at timestamp from stage content.
    ///
    /// Extracts the `updated_at` field from YAML frontmatter using proper parsing.
    fn get_stage_started_at(content: &str) -> chrono::DateTime<chrono::Utc> {
        // Use proper YAML parsing
        if let Ok(yaml) = extract_yaml_frontmatter(content) {
            if let Some(updated_at) = yaml.get("updated_at").and_then(|v| v.as_str()) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated_at) {
                    return dt.with_timezone(&chrono::Utc);
                }
            }
        }
        chrono::Utc::now()
    }

    /// Get session PID from session file.
    ///
    /// Extracts the `pid` field from session YAML frontmatter using proper parsing.
    fn get_session_pid(sessions_dir: &Path, session_id: Option<&str>) -> Option<u32> {
        let session_id = session_id?;

        // Try direct path first
        let session_path = sessions_dir.join(format!("{session_id}.md"));
        let content = if session_path.exists() {
            fs::read_to_string(&session_path).ok()?
        } else {
            // Search for matching file
            let entries = fs::read_dir(sessions_dir).ok()?;
            let mut found_content = None;
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem == session_id || stem.contains(session_id) {
                        found_content = fs::read_to_string(&path).ok();
                        break;
                    }
                }
            }
            found_content?
        };

        // Parse PID from frontmatter using proper YAML parsing
        let yaml = extract_yaml_frontmatter(&content).ok()?;
        yaml.get("pid")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
    }

    /// Handle a client connection.
    fn handle_client_connection(
        mut stream: UnixStream,
        shutdown_flag: Arc<AtomicBool>,
        status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
        log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    ) -> Result<()> {
        loop {
            let request: Request = match read_message(&mut stream) {
                Ok(req) => req,
                Err(_) => {
                    // Client disconnected or error reading
                    break;
                }
            };

            match request {
                Request::Ping => {
                    write_message(&mut stream, &Response::Pong)?;
                }
                Request::Stop => {
                    write_message(&mut stream, &Response::Ok)?;
                    shutdown_flag.store(true, Ordering::Relaxed);
                    break;
                }
                Request::SubscribeStatus => {
                    if let Ok(stream_clone) = stream.try_clone() {
                        match status_subscribers.lock() {
                            Ok(mut subs) => {
                                subs.push(stream_clone);
                                write_message(&mut stream, &Response::Ok)?;
                            }
                            Err(_) => {
                                write_message(
                                    &mut stream,
                                    &Response::Error {
                                        message: "Failed to acquire subscriber lock".to_string(),
                                    },
                                )?;
                            }
                        }
                    } else {
                        write_message(
                            &mut stream,
                            &Response::Error {
                                message: "Failed to clone stream".to_string(),
                            },
                        )?;
                    }
                }
                Request::SubscribeLogs => {
                    if let Ok(stream_clone) = stream.try_clone() {
                        match log_subscribers.lock() {
                            Ok(mut subs) => {
                                subs.push(stream_clone);
                                write_message(&mut stream, &Response::Ok)?;
                            }
                            Err(_) => {
                                write_message(
                                    &mut stream,
                                    &Response::Error {
                                        message: "Failed to acquire subscriber lock".to_string(),
                                    },
                                )?;
                            }
                        }
                    } else {
                        write_message(
                            &mut stream,
                            &Response::Error {
                                message: "Failed to clone stream".to_string(),
                            },
                        )?;
                    }
                }
                Request::Unsubscribe => {
                    write_message(&mut stream, &Response::Ok)?;
                    break;
                }
                Request::StartWithConfig(_config) => {
                    // Stub for stage-2-daemon-server to implement config application
                    write_message(
                        &mut stream,
                        &Response::Error {
                            message: "StartWithConfig not yet implemented".to_string(),
                        },
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Request graceful shutdown of the daemon.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
    }

    /// Clean up socket and PID files.
    fn cleanup(&self) -> Result<()> {
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path).context("Failed to remove socket file")?;
        }
        if self.pid_path.exists() {
            fs::remove_file(&self.pid_path).context("Failed to remove PID file")?;
        }
        Ok(())
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_daemon_server() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();

        let server = DaemonServer::new(work_dir);

        assert_eq!(server.socket_path, work_dir.join("orchestrator.sock"));
        assert_eq!(server.pid_path, work_dir.join("orchestrator.pid"));
        assert_eq!(server.log_path, work_dir.join("orchestrator.log"));
    }

    #[test]
    fn test_is_running_no_pid_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();

        assert!(!DaemonServer::is_running(work_dir));
    }

    #[test]
    fn test_read_pid_valid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();
        let pid_path = work_dir.join("orchestrator.pid");

        fs::write(&pid_path, "12345").expect("Failed to write PID file");

        let pid = DaemonServer::read_pid(work_dir);
        assert_eq!(pid, Some(12345));
    }

    #[test]
    fn test_read_pid_invalid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();
        let pid_path = work_dir.join("orchestrator.pid");

        fs::write(&pid_path, "not-a-number").expect("Failed to write PID file");

        let pid = DaemonServer::read_pid(work_dir);
        assert_eq!(pid, None);
    }

    #[test]
    fn test_read_pid_no_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();

        let pid = DaemonServer::read_pid(work_dir);
        assert_eq!(pid, None);
    }

    #[test]
    fn test_shutdown_flag() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();

        let server = DaemonServer::new(work_dir);
        assert!(!server.shutdown_flag.load(Ordering::Relaxed));

        server.shutdown();
        assert!(server.shutdown_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_parse_stage_frontmatter_valid() {
        let content = r#"---
id: stage-1
name: Test Stage
status: executing
session: session-123
---

# Stage content
"#;

        let result = DaemonServer::parse_stage_frontmatter(content);
        assert!(result.is_some());

        let (id, name, status, session) = result.unwrap();
        assert_eq!(id, "stage-1");
        assert_eq!(name, "Test Stage");
        assert_eq!(status, "executing");
        assert_eq!(session, Some("session-123".to_string()));
    }

    #[test]
    fn test_parse_stage_frontmatter_no_session() {
        let content = r#"---
id: stage-2
name: Another Stage
status: pending
session: ~
---

# Stage content
"#;

        let result = DaemonServer::parse_stage_frontmatter(content);
        assert!(result.is_some());

        let (id, name, status, session) = result.unwrap();
        assert_eq!(id, "stage-2");
        assert_eq!(name, "Another Stage");
        assert_eq!(status, "pending");
        assert!(session.is_none());
    }

    #[test]
    fn test_parse_stage_frontmatter_missing_frontmatter() {
        let content = "# No frontmatter here";
        let result = DaemonServer::parse_stage_frontmatter(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_collect_status_empty_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();

        let result = DaemonServer::collect_status(work_dir);
        assert!(result.is_ok());

        match result.unwrap() {
            Response::StatusUpdate {
                stages_executing,
                stages_pending,
                stages_completed,
                stages_blocked,
            } => {
                assert!(stages_executing.is_empty());
                assert!(stages_pending.is_empty());
                assert!(stages_completed.is_empty());
                assert!(stages_blocked.is_empty());
            }
            _ => panic!("Expected StatusUpdate response"),
        }
    }

    #[test]
    fn test_collect_status_with_stages() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path();
        let stages_dir = work_dir.join("stages");
        fs::create_dir_all(&stages_dir).expect("Failed to create stages dir");

        // Create a pending stage
        let pending_stage = r#"---
id: stage-pending
name: Pending Stage
status: pending
session: ~
---
"#;
        fs::write(stages_dir.join("stage-pending.md"), pending_stage)
            .expect("Failed to write stage");

        // Create an executing stage
        let executing_stage = r#"---
id: stage-executing
name: Executing Stage
status: executing
session: session-1
---
"#;
        fs::write(stages_dir.join("stage-executing.md"), executing_stage)
            .expect("Failed to write stage");

        // Create a completed stage
        let completed_stage = r#"---
id: stage-completed
name: Completed Stage
status: completed
session: ~
---
"#;
        fs::write(stages_dir.join("stage-completed.md"), completed_stage)
            .expect("Failed to write stage");

        let result = DaemonServer::collect_status(work_dir);
        assert!(result.is_ok());

        match result.unwrap() {
            Response::StatusUpdate {
                stages_executing,
                stages_pending,
                stages_completed,
                stages_blocked,
            } => {
                assert_eq!(stages_executing.len(), 1);
                assert_eq!(stages_executing[0].id, "stage-executing");
                assert_eq!(stages_pending.len(), 1);
                assert!(stages_pending.contains(&"stage-pending".to_string()));
                assert_eq!(stages_completed.len(), 1);
                assert!(stages_completed.contains(&"stage-completed".to_string()));
                assert!(stages_blocked.is_empty());
            }
            _ => panic!("Expected StatusUpdate response"),
        }
    }

    #[test]
    fn test_detect_worktree_status_no_worktree() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_root = temp_dir.path();

        // When worktree doesn't exist, should return None
        let status = DaemonServer::detect_worktree_status("nonexistent-stage", repo_root);
        assert!(status.is_none());
    }

    #[test]
    fn test_detect_worktree_status_active() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_root = temp_dir.path();

        // Create a worktree directory (without git operations)
        let worktree_path = repo_root.join(".worktrees").join("test-stage");
        fs::create_dir_all(&worktree_path).expect("Failed to create worktree dir");

        // Create a .git file pointing to a gitdir (simulating a worktree)
        let git_file = worktree_path.join(".git");
        fs::write(&git_file, "gitdir: /nonexistent/path").expect("Failed to write .git file");

        // Since this is not a real git repo, is_manually_merged will return false
        // and there's no MERGE_HEAD, so status should be Active
        let status = DaemonServer::detect_worktree_status("test-stage", repo_root);
        assert_eq!(status, Some(WorktreeStatus::Active));
    }

    #[test]
    fn test_is_manually_merged_no_git_repo() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_root = temp_dir.path();

        // When not in a git repo, is_manually_merged should gracefully return false
        let result = DaemonServer::is_manually_merged("test-stage", repo_root);
        assert!(!result);
    }

    // Note: Testing is_manually_merged with a real git repo and merged branches
    // requires complex setup and is better suited for e2e tests.
    // The function:
    // 1. Gets the default branch (main/master)
    // 2. Checks if loom/{stage_id} is in `git branch --merged {target}`
    // 3. Returns true if the branch has been merged, false otherwise
}
