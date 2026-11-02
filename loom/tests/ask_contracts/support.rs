//! Builders for the `loom ask` contracts: a scratch scene, recording fake
//! provider binaries, and a runner whose output goes to files, never pipes.

use std::fs::{self, File};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

pub const QUESTION: &str = "What is the final word of the document?";
pub const DEADLINE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

pub type Row = (String, Vec<String>, Vec<String>);

pub struct Scene {
    _dir: TempDir,
    root: PathBuf,
}

impl Scene {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create scene root");
        let root = dir.path().canonicalize().expect("canonicalize scene root");
        for sub in ["project", "bin", "rec", "out"] {
            fs::create_dir(root.join(sub)).expect("create scene subdirectory");
        }
        Scene { _dir: dir, root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    pub fn write(&self, rel: &str, content: impl AsRef<[u8]>) -> PathBuf {
        write_under(&self.project(), rel, content)
    }

    pub fn write_outside(&self, rel: &str, content: impl AsRef<[u8]>) -> PathBuf {
        write_under(&self.root, rel, content)
    }

    pub fn symlink(&self, rel: &str, target: &str) {
        symlink(target, self.project().join(rel)).expect("create symlink");
    }

    /// Writes an executable `bin/<name>` that records its run, then runs `body`.
    pub fn fake(&self, name: &str, body: &str) -> PathBuf {
        let (runs, argv) = (self.sh("runs"), self.sh("argv"));
        let (cwd, env) = (self.sh("cwd"), self.sh("env"));
        let script = format!(
            "#!/bin/sh\nprintf 'run\\n' >> {runs}\n: > {argv}\n\
             for a in \"$@\"; do printf '%s\\n' \"$a\" >> {argv}; done\n\
             pwd -P > {cwd}\nenv > {env}\n{body}\n"
        );
        let path = self.root.join("bin").join(name);
        fs::write(&path, script).expect("write fake binary");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod fake");
        path
    }

    pub fn answering(&self, name: &str, output: &str) -> PathBuf {
        self.set_output(output);
        let body = format!("cat > {}\ncat {}", self.sh("stdin"), self.sh("output"));
        self.fake(name, &body)
    }

    pub fn serve_claude(&self) -> PathBuf {
        self.answering("claude", &capture("ask-claude.json"))
    }

    /// Fake claude that floods both streams before it reads stdin, and fails
    /// unless the payload's last line is `marker`.
    pub fn noisy_claude(&self, marker: &str) -> PathBuf {
        self.set_output(&capture("ask-claude.json"));
        let (stdin, output) = (self.sh("stdin"), self.sh("output"));
        // The shell's printf: uutils `yes | head -c` fits 200 KiB in an unread
        // pipe, so it would never block and proves nothing about draining.
        let kib = "\\n".repeat(1024);
        let flood = format!("i=0\nwhile [ $i -lt 200 ]; do printf '{kib}'; i=$((i+1)); done");
        self.fake(
            "claude",
            &format!(
                "flood() {{\n{flood}\n}}\nflood\nflood >&2\ncat > {stdin}\n\
                 grep -qF '{marker}' {stdin} || exit 3\ncat {output}"
            ),
        )
    }

    pub fn set_output(&self, output: &str) {
        fs::write(self.rec("output"), output).expect("write fake output");
    }

    pub fn rec(&self, file: &str) -> PathBuf {
        self.root.join("rec").join(file)
    }

    /// The recording path as a single-quoted shell word, for fake bodies.
    pub fn sh(&self, file: &str) -> String {
        format!("'{}'", self.rec(file).display())
    }

    pub fn runs(&self) -> usize {
        fs::read_to_string(self.rec("runs")).map_or(0, |text| text.lines().count())
    }

    pub fn argv(&self) -> Vec<String> {
        self.recorded("argv").lines().map(str::to_owned).collect()
    }

    pub fn recorded_cwd(&self) -> PathBuf {
        PathBuf::from(self.recorded("cwd").trim_end_matches('\n'))
    }

    pub fn env_lines(&self) -> Vec<String> {
        self.recorded("env").lines().map(str::to_owned).collect()
    }

    pub fn stdin(&self) -> String {
        self.recorded("stdin")
    }

    fn recorded(&self, file: &str) -> String {
        fs::read_to_string(self.rec(file))
            .unwrap_or_else(|e| panic!("fake never wrote {file}: {e}"))
    }

    /// Loom's own stdout and stderr go to files, never pipes.
    pub fn loom(&self, args: &[&str]) -> Command {
        let mut command = crate::helpers::loom_cmd();
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let dirs = std::iter::once(self.root.join("bin")).chain(std::env::split_paths(&inherited));
        command
            .args(args)
            .current_dir(self.project())
            .env("PATH", std::env::join_paths(dirs).expect("join PATH"))
            .stdin(Stdio::null())
            .stdout(File::create(self.out("stdout")).expect("create stdout file"))
            .stderr(File::create(self.out("stderr")).expect("create stderr file"));
        command
    }

    fn out(&self, file: &str) -> PathBuf {
        self.root.join("out").join(file)
    }

    pub fn start(&self, mut command: Command) -> Running {
        let child = command.spawn().expect("spawn loom");
        Running {
            child,
            stdout: self.out("stdout"),
            stderr: self.out("stderr"),
        }
    }

    pub fn run(&self, args: &[&str]) -> Finished {
        self.run_within(args, DEADLINE)
    }

    pub fn run_within(&self, args: &[&str], limit: Duration) -> Finished {
        self.start(self.loom(args)).wait_within(limit)
    }

    /// `loom ask --question <QUESTION> <flags> --paths <paths>`.
    pub fn ask(&self, flags: &[&str], paths: &[&str]) -> Finished {
        self.run(&ask_args(flags, paths))
    }

    pub fn eval(&self, cases: &str) -> Finished {
        self.run(&["ask", "--eval", cases])
    }
}

fn write_under(base: &Path, rel: &str, content: impl AsRef<[u8]>) -> PathBuf {
    let path = base.join(rel);
    fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parents");
    fs::write(&path, content).expect("write file");
    path
}

pub fn ask_args<'a>(flags: &[&'a str], paths: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["ask", "--question", QUESTION];
    args.extend_from_slice(flags);
    args.push("--paths");
    args.extend_from_slice(paths);
    args
}

pub struct Running {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Running {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Waits for loom to exit; past the deadline it kills loom and panics.
    pub fn wait_within(mut self, limit: Duration) -> Finished {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll loom") {
                return self.finished(status);
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                let stderr = read_lossy(&self.stderr);
                panic!("loom still running after {limit:?}; stderr:\n{stderr}");
            }
            sleep(POLL);
        }
    }

    fn finished(&self, status: ExitStatus) -> Finished {
        Finished {
            code: status.code(),
            stdout: read_lossy(&self.stdout),
            stderr: read_lossy(&self.stderr),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn read_lossy(path: &Path) -> String {
    String::from_utf8_lossy(&fs::read(path).unwrap_or_default()).into_owned()
}

pub struct Finished {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Finished {
    #[track_caller]
    pub fn assert_exit(&self, want: i32) {
        let (code, stdout, stderr) = (self.code, &self.stdout, &self.stderr);
        assert_eq!(code, Some(want), "stdout:\n{stdout}\nstderr:\n{stderr}");
    }

    /// Exit 1 with nothing on stdout: no answer was printed.
    #[track_caller]
    pub fn assert_failed(&self) {
        self.assert_exit(1);
        assert!(self.stdout.trim().is_empty(), "stdout:\n{}", self.stdout);
    }

    #[track_caller]
    pub fn assert_refused(&self, phrase: &str) {
        self.assert_failed();
        let stderr = &self.stderr;
        assert!(stderr.contains(phrase), "no {phrase:?} in:\n{stderr}");
    }

    #[track_caller]
    pub fn assert_stdout_has(&self, needle: &str) {
        let stdout = &self.stdout;
        assert!(stdout.contains(needle), "no {needle:?} in:\n{stdout}");
    }

    #[track_caller]
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", self.stdout))
    }
}

pub fn read_pid(path: &Path) -> u32 {
    let text = fs::read_to_string(path).expect("fake wrote its pid file");
    text.trim().parse().expect("pid file holds a pid")
}

/// Returns once the pid file is complete, loom has exited, or 20 s have passed.
pub fn wait_for_pid_file(running: &mut Running, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let complete = fs::read_to_string(path).is_ok_and(|text| text.ends_with('\n'));
        if complete || running.child.try_wait().expect("poll loom").is_some() {
            return;
        }
        sleep(POLL);
    }
}

pub fn assert_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while loom::process::is_process_alive(pid) {
        assert!(Instant::now() < deadline, "process {pid} is still alive");
        sleep(POLL);
    }
}

pub fn capture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/ask/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Exactly `size` ASCII bytes ending in `\n<last_line>\n`.
pub fn sized_doc(size: usize, last_line: &str) -> String {
    let tail = format!("\n{last_line}\n");
    let pad = size - tail.len();
    let filler = "padding line\n".repeat(pad / 13 + 1);
    let doc = format!("{}{tail}", &filler[..pad]);
    assert_eq!(doc.len(), size);
    doc
}

/// Inserts `row` before the single line containing `needle`.
pub fn insert_before(text: &str, needle: &str, row: &str) -> String {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.contains(needle) {
            out.push(row);
        }
        out.push(line);
    }
    assert_eq!(out.len(), text.lines().count() + 1, "no {needle:?}");
    out.join("\n") + "\n"
}

/// Removes the single line containing `needle`.
pub fn drop_line(text: &str, needle: &str) -> String {
    let kept: Vec<&str> = text.lines().filter(|line| !line.contains(needle)).collect();
    assert_eq!(kept.len() + 1, text.lines().count(), "need one {needle:?}");
    kept.join("\n") + "\n"
}

pub fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let at = argv.iter().position(|arg| arg == flag)?;
    argv.get(at + 1).map(String::as_str)
}

pub fn has_flag(argv: &[String], flag: &str) -> bool {
    argv.iter().any(|arg| arg == flag)
}

#[track_caller]
pub fn assert_tokens(v: &Value, input: u64, cache_read: u64, cache_write: u64, output: u64) {
    let want = [
        ("input_tokens", input),
        ("cache_read_tokens", cache_read),
        ("cache_write_tokens", cache_write),
        ("output_tokens", output),
    ];
    for (key, value) in want {
        assert_eq!(v[key].as_u64(), Some(value), "{key} in {v}");
    }
}

pub fn row(id: &str, must_contain: &[&str], must_not_contain: &[&str]) -> Row {
    let owned = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    (id.to_owned(), owned(must_contain), owned(must_not_contain))
}

/// `total` rows; the first `satisfied` accept the fixed answer `ok`.
pub fn cases_satisfying(total: usize, satisfied: usize) -> Vec<Row> {
    (0..total)
        .map(|i| {
            let must = ["missing-from-the-answer", "ok"][usize::from(i < satisfied)];
            row(&format!("case-{i}"), &[must], &[])
        })
        .collect()
}

pub fn write_cases(scene: &Scene, file: &str, rows: &[Row], doc: &str) {
    let cases: Vec<Value> = rows
        .iter()
        .map(|(id, must, forbid)| {
            json!({
                "id": id,
                "question": QUESTION,
                "paths": [doc],
                "must_contain": must,
                "must_not_contain": forbid,
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&cases).expect("serialize cases");
    scene.write(file, text);
}
