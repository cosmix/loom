//! Completion-marker instructions for autonomous Claude sessions.

use std::path::Path;

/// The directory whose write permission allows the completion marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerHome {
    Workspace,
    PlanSession,
}

/// A single shell argument, protected from option parsing and shell syntax.
pub(crate) fn marker_command(marker: &Path) -> String {
    format!(
        "touch -- {}",
        shell_escape::escape(marker.to_string_lossy())
    )
}

/// Instruct Claude to create `marker` as its final action.
pub fn completion_instruction(marker: &Path, home: MarkerHome) -> String {
    let location = match home {
        MarkerHome::Workspace => {
            "That path is inside the repo's gitignored `.loom/work/` because the agent sandbox \
             mounts /tmp read-only; creating this one marker is the sanctioned exception to the rule against \
             writing under `.loom/work/` directly."
        }
        MarkerHome::PlanSession => {
            "That path is this session's completion-marker directory, the one place outside the plan this session may write; \
             creating this one marker is the only write it is for."
        }
    };
    format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom pressure`; no human will end it for you. \
         When the task is FULLY complete and the plan file is fully updated (after every subagent has finished), \
         your FINAL action MUST be to run exactly this shell command and nothing after it: {}. \
         Do not run it earlier. {location} Once that file exists the driver closes this session.",
        marker_command(marker)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn marker_command_passes_one_literal_path_to_touch() {
        for path in [
            "/project with spaces/marker",
            "/project'quote/marker",
            "/project;$(echo bad)/marker",
            "-option",
        ] {
            let command = marker_command(Path::new(path));
            let script = format!("touch() {{ printf '%s\\0' \"$@\"; }}; {command}");
            let output = Command::new("sh").arg("-c").arg(script).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, format!("--\0{path}\0").as_bytes());
        }
    }
}
