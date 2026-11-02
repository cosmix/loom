//! Retire Codex attempts when the process gets a termination signal.
//!
//! An attempt leads its own process group, so the terminal's SIGINT never
//! reaches it and this process dying by default leaves it running. A signal
//! handler holds a [`CodexRetirement`] and retires every run started through
//! it, on any Unix: `killpg` is all this needs. A SIGKILL of this process runs
//! no handler and still leaves the attempt running. `PR_SET_PDEATHSIG` would
//! cover that on Linux only, and for the group leader only.

use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use anyhow::{bail, Result};

use super::{cancel_groups, start_run, CodexRun, RunControl};

/// The runs one signal handler retires.
#[derive(Clone, Default)]
pub struct CodexRetirement(Arc<Mutex<Enrolled>>);

#[derive(Default)]
struct Enrolled {
    retiring: bool,
    runs: Vec<Weak<Mutex<RunControl>>>,
}

impl CodexRetirement {
    /// [`super::spawn_codex_with_fallback`] for a run [`Self::retire`] stops.
    /// The run is enrolled before its first attempt starts, so a signal at any
    /// point either stops the attempt or refuses it.
    pub fn spawn_with_fallback(
        &self,
        codex_path: &Path,
        repo_root: &Path,
        skill: &str,
        log_path: &Path,
        candidates: Vec<String>,
        effort: &str,
    ) -> Result<CodexRun> {
        let control = self.enrol()?;
        start_run(
            control, codex_path, repo_root, skill, log_path, candidates, effort,
        )
    }

    fn enrol(&self) -> Result<Arc<Mutex<RunControl>>> {
        let mut enrolled = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if enrolled.retiring {
            bail!("codex run refused: this process is retiring its runs");
        }
        let control = Arc::default();
        enrolled.runs.retain(|run| run.strong_count() > 0);
        enrolled.runs.push(Arc::downgrade(&control));
        Ok(control)
    }

    /// Stop every enrolled run's process groups and refuse any later run. The
    /// first failure is returned after every run has been tried.
    pub fn retire(&self) -> Result<()> {
        let mut enrolled = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        enrolled.retiring = true;
        let mut outcome = Ok(());
        for run in enrolled.runs.iter().filter_map(Weak::upgrade) {
            let stopped = cancel_groups(&run);
            if outcome.is_ok() {
                outcome = stopped;
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retiring_stops_a_run_refuses_its_fallback_and_refuses_new_runs() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("exec"),
            "echo attempt >> attempts\necho started > started\nsleep 30\n",
        )
        .unwrap();
        let retirement = CodexRetirement::default();
        let spawn = || {
            retirement.spawn_with_fallback(
                Path::new("/bin/sh"),
                repo.path(),
                "$pressure plan.md",
                &repo.path().join("codex.log"),
                vec!["first".to_owned(), "fallback".to_owned()],
                "high",
            )
        };
        let run = spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !repo.path().join("started").exists() {
            assert!(std::time::Instant::now() < deadline, "codex never started");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        retirement.retire().unwrap();

        assert!(!run.join().unwrap().status.success());
        assert_eq!(
            std::fs::read_to_string(repo.path().join("attempts")).unwrap(),
            "attempt\n"
        );
        assert!(spawn().is_err(), "a retiring process starts no more runs");
    }
}
