//! Lines a manual-mode run prints for a session it leaves the operator to
//! start by hand. The last line says the session runs unconfined.

use std::path::Path;

use crate::testrun::command::{shell_quote, shell_word};

const CONFINEMENT_NOTICE: &str = "  Confinement: none. A session started this way runs outside the loom wrapper: no settings capsule (sandbox, write denies, hooks) and no environment allowlist.";

/// Lines printed for a stage session: setup header, worktree, signal, the
/// start command with its identity variables, and the confinement notice.
pub(super) fn manual_stage_start_lines(
    stage_id: &str,
    worktree: &Path,
    session_id: &str,
    signal: &Path,
    work_dir: &Path,
) -> Vec<String> {
    let absolute_work_dir = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let prompt = format!(
        "Read the signal file at {} and execute the assigned stage work.",
        signal.display()
    );
    vec![
        format!("Manual mode: Session setup for stage '{stage_id}'"),
        format!("  Worktree: {}", worktree.display()),
        format!("  Signal: {}", signal.display()),
        format!(
            "  To start: cd {} && LOOM_STAGE_ID={} LOOM_SESSION_ID={} LOOM_WORK_DIR={} claude {}",
            shell_word(&worktree.display().to_string()),
            shell_word(stage_id),
            shell_word(session_id),
            shell_word(&absolute_work_dir.display().to_string()),
            shell_quote(&prompt)
        ),
        CONFINEMENT_NOTICE.to_string(),
    ]
}

/// Lines printed for a knowledge stage session, which runs in the checkout.
pub(super) fn manual_knowledge_start_lines(
    stage_id: &str,
    repo_root: &Path,
    signal: &Path,
) -> Vec<String> {
    let prompt = format!(
        "Read the signal file at {} and execute the assigned stage work.",
        signal.display()
    );
    vec![
        format!("Manual mode: Session setup for knowledge stage '{stage_id}'"),
        format!("  Directory: {}", repo_root.display()),
        format!("  Signal: {}", signal.display()),
        format!(
            "  To start: cd {} && claude {}",
            shell_word(&repo_root.display().to_string()),
            shell_quote(&prompt)
        ),
        CONFINEMENT_NOTICE.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn line_after_start(lines: &[String]) -> (&String, &String) {
        let i = lines
            .iter()
            .position(|l| l.starts_with("  To start:"))
            .expect("start line");
        (&lines[i], &lines[i + 1])
    }

    #[test]
    fn the_stage_start_command_is_followed_by_the_confinement_notice() {
        let lines = manual_stage_start_lines(
            "s1",
            Path::new("/nope/wt"),
            "sess-1",
            Path::new("/nope/sig.md"),
            Path::new("/nope/work"),
        );
        assert_eq!(lines.len(), 5);
        let (start, next) = line_after_start(&lines);
        assert_eq!(next, CONFINEMENT_NOTICE);
        for needle in [
            "LOOM_STAGE_ID=",
            "LOOM_SESSION_ID=",
            "LOOM_WORK_DIR=",
            "claude 'Read the signal file at",
        ] {
            assert!(start.contains(needle), "{needle}");
        }
        assert_eq!(
            lines,
            [
                "Manual mode: Session setup for stage 's1'",
                "  Worktree: /nope/wt",
                "  Signal: /nope/sig.md",
                "  To start: cd /nope/wt && LOOM_STAGE_ID=s1 LOOM_SESSION_ID=sess-1 \
                 LOOM_WORK_DIR=/nope/work claude 'Read the signal file at /nope/sig.md \
                 and execute the assigned stage work.'",
                CONFINEMENT_NOTICE,
            ]
        );
    }

    #[test]
    fn the_knowledge_start_command_is_followed_by_the_confinement_notice() {
        let lines =
            manual_knowledge_start_lines("k1", Path::new("/nope/repo"), Path::new("/nope/sig.md"));
        assert_eq!(lines.len(), 5);
        let (start, next) = line_after_start(&lines);
        assert_eq!(next, CONFINEMENT_NOTICE);
        assert!(start.contains("cd /nope/repo && claude 'Read the signal file at"));
        assert_eq!(
            lines,
            [
                "Manual mode: Session setup for knowledge stage 'k1'",
                "  Directory: /nope/repo",
                "  Signal: /nope/sig.md",
                "  To start: cd /nope/repo && claude 'Read the signal file at /nope/sig.md \
                 and execute the assigned stage work.'",
                CONFINEMENT_NOTICE,
            ]
        );
    }

    #[test]
    fn the_notice_names_what_the_session_lacks() {
        for word in ["capsule", "sandbox", "allowlist"] {
            assert!(CONFINEMENT_NOTICE.contains(word), "{word}");
        }
        assert!(CONFINEMENT_NOTICE.starts_with("  Confinement: none."));
    }

    fn run_printed_start(lines: &[String], cwd: &Path) -> Vec<String> {
        let (start, _) = line_after_start(lines);
        let command = start.strip_prefix("  To start: ").unwrap();
        let script = format!(
            "claude() {{ printf '%s\\0' \"$PWD\" \"$LOOM_STAGE_ID\" \"$LOOM_SESSION_ID\" \"$LOOM_WORK_DIR\" \"$1\"; }}; {command}"
        );
        let result = Command::new("sh")
            .arg("-c")
            .arg(script)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result
            .stdout
            .split(|byte| *byte == 0)
            .filter(|word| !word.is_empty())
            .map(|word| String::from_utf8(word.to_vec()).unwrap())
            .collect()
    }

    #[test]
    fn stage_command_keeps_hostile_paths_and_prompt_literal() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("executed");
        let hostile = "a b ' $HOME `touch executed` $(touch executed)\nend";
        let worktree = temp.path().join(format!("worktree {hostile}"));
        let work_dir = temp.path().join(format!("state {hostile}"));
        let signal = temp.path().join(format!("signal {hostile}"));
        std::fs::create_dir(&worktree).unwrap();
        std::fs::create_dir(&work_dir).unwrap();
        let lines = manual_stage_start_lines("stage-1", &worktree, "session-1", &signal, &work_dir);
        let words = run_printed_start(&lines, temp.path());
        assert_eq!(
            words,
            [
                worktree.display().to_string(),
                "stage-1".to_string(),
                "session-1".to_string(),
                work_dir.display().to_string(),
                format!(
                    "Read the signal file at {} and execute the assigned stage work.",
                    signal.display()
                ),
            ]
        );
        assert!(!marker.exists());
        assert!(!worktree.join("executed").exists());
    }

    #[test]
    fn knowledge_command_keeps_hostile_checkout_and_prompt_literal() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("executed");
        let hostile = "a b ' $HOME `touch executed` $(touch executed)\nend";
        let repo = temp.path().join(format!("checkout {hostile}"));
        let signal = temp.path().join(format!("signal {hostile}"));
        std::fs::create_dir(&repo).unwrap();
        let lines = manual_knowledge_start_lines("knowledge-1", &repo, &signal);
        let words = run_printed_start(&lines, temp.path());
        assert_eq!(words[0], repo.display().to_string());
        assert_eq!(
            words[1],
            format!(
                "Read the signal file at {} and execute the assigned stage work.",
                signal.display()
            )
        );
        assert!(!marker.exists());
        assert!(!repo.join("executed").exists());
    }
}
