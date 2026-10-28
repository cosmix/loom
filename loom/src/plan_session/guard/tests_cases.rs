//! Temporary-directory, link, path-shape and payload edge-case tests of the planning PreToolUse
//! guard.

use std::ffi::CString;
use std::fs;
use std::os::unix::{ffi::OsStrExt, fs::symlink};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};
use tempfile::TempDir;

use super::claude_tmp_dir;
use super::tests::{assert_deny, payload, pressure, Fixture};
use crate::plan_session::kind::SessionKind;

macro_rules! case {
    ($name:ident, $kind:expr, $f:ident, $body:block) => {
        #[test]
        fn $name() -> Result<()> {
            let $f = Fixture::new($kind)?;
            $body
            Ok(())
        }
    };
}

macro_rules! temp_cases {
    ($name:ident, $kind:expr) => {
        mod $name {
            use super::*;
            case!(write_deferred, $kind, f, {
                let dir = temporary_workspace()?;
                f.write(&dir.path().join("x.md"), false);
            });
            case!(symlink_denied, $kind, f, {
                let dir = temporary_workspace()?;
                fs::create_dir(dir.path().join("real"))?;
                symlink(dir.path().join("real"), dir.path().join("link"))?;
                f.write(&dir.path().join("link/x.md"), true);
            });
        }
    };
}

fn temporary_workspace() -> Result<TempDir> {
    let dir = claude_tmp_dir().expect("absolute Claude temporary directory");
    fs::create_dir_all(&dir)?;
    Ok(tempfile::tempdir_in(dir)?)
}

temp_cases!(temporary_authoring, SessionKind::Authoring);
temp_cases!(temporary_summary, SessionKind::Summary);
temp_cases!(temporary_pressure, pressure());

case!(deny_json_has_exact_shape, SessionKind::Authoring, f, {
    let input = payload("Write", &f.root.join("doc/plans/IN_PROGRESS-PLAN-b.md"));
    assert_eq!(
        assert_deny(f.run(&serde_json::to_vec(&input)?)),
        json!({
            "hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny",
                "permissionDecisionReason":
                    "loom plan: doc/plans/IN_PROGRESS-PLAN-b.md is locked (running plan)"}
        })
    );
});

case!(
    all_write_tools_extract_their_path_field,
    SessionKind::Authoring,
    f,
    {
        for tool in ["Write", "Edit", "MultiEdit", "NotebookEdit"] {
            f.check(payload(tool, &f.root.join("doc/product/x.md")), false);
            f.check(payload(tool, &f.root.join("README.md")), true);
        }
    }
);

fn mid_session_lock(kind: SessionKind) -> Result<()> {
    let f = Fixture::new(kind)?;
    let plan = f.root.join("doc/plans/PLAN-a.md");
    let locked = f.root.join("doc/plans/IN_PROGRESS-PLAN-a.md");
    let bundle = f.root.join("doc/plans/briefs/a/y.md");
    f.write(&plan, false);
    f.write(&bundle, false);
    fs::rename(plan, &locked)?;
    f.write(&locked, true);
    f.write(&bundle, true);
    Ok(())
}

#[test]
fn authoring_observes_mid_session_lock() -> Result<()> {
    mid_session_lock(SessionKind::Authoring)
}
#[test]
fn pressure_observes_mid_session_lock() -> Result<()> {
    mid_session_lock(pressure())
}

case!(symlinked_doc_directory_denied, SessionKind::Authoring, f, {
    symlink(f.root.join("doc/product"), f.root.join("doc/linked"))?;
    f.write(&f.root.join("doc/linked/x.md"), true);
});
case!(
    symlinked_draft_component_denied,
    SessionKind::Authoring,
    f,
    {
        symlink(f.root.join("doc/product"), f.runtime.drafts.join("linked"))?;
        f.write(&f.runtime.drafts.join("linked/x.md"), true);
    }
);
case!(
    summary_symlinked_draft_component_denied,
    SessionKind::Summary,
    f,
    {
        symlink(f.root.join("doc/product"), f.runtime.drafts.join("linked"))?;
        f.write(&f.runtime.drafts.join("linked/x.md"), true);
    }
);
case!(symlinked_omissions_denied, pressure(), f, {
    symlink(
        f.root.join("doc/plans/PLAN-a.md"),
        f.root.join("planning-omissions.md"),
    )?;
    f.write(&f.root.join("planning-omissions.md"), true);
});

fn make_fifo(path: &Path) {
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: path is NUL-terminated and remains valid for the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
}

case!(fifo_target_denied, SessionKind::Authoring, f, {
    let path = f.root.join("doc/product/pipe");
    make_fifo(&path);
    f.write(&path, true);
});
case!(fifo_omissions_denied, pressure(), f, {
    let path = f.root.join("planning-omissions.md");
    make_fifo(&path);
    f.write(&path, true);
});
case!(directory_omissions_denied, pressure(), f, {
    let path = f.root.join("planning-omissions.md");
    fs::create_dir(&path)?;
    f.write(&path, true);
});
case!(directory_target_denied, SessionKind::Authoring, f, {
    f.write(&f.root.join("doc/product"), true);
});
case!(hardlinked_target_denied, SessionKind::Authoring, f, {
    let path = f.root.join("doc/product/x.md");
    fs::hard_link(f.root.join("doc/plans/PLAN-a.md"), &path)?;
    f.write(&path, true);
});
case!(relative_path_uses_cwd, SessionKind::Authoring, f, {
    let mut input = payload("Write", Path::new("x.md"));
    input["cwd"] = json!(f.root.join("doc/product"));
    f.check(input, false);
});
case!(
    relative_path_without_cwd_uses_root,
    SessionKind::Authoring,
    f,
    {
        f.write(Path::new("doc/product/x.md"), false);
    }
);
case!(
    parent_over_real_directory_deferred,
    SessionKind::Authoring,
    f,
    {
        f.write(&f.root.join("doc/notes/../x.md"), false);
    }
);
case!(parent_over_symlink_denied, SessionKind::Authoring, f, {
    fs::remove_dir(f.root.join("doc/notes"))?;
    symlink(&f.home, f.root.join("doc/notes"))?;
    f.write(&f.root.join("doc/notes/../x.md"), true);
});
case!(
    parent_escape_out_of_doc_denied,
    SessionKind::Authoring,
    f,
    {
        f.write(&f.root.join("doc/../README.md"), true);
    }
);
case!(parent_above_root_denied, SessionKind::Authoring, f, {
    f.write(Path::new("/../../doc/x.md"), true);
});

macro_rules! bad_payload {
    ($name:ident, $field:literal, $value:expr) => {
        case!($name, SessionKind::Authoring, f, {
            let mut input = payload("Write", &f.root.join("doc/product/x.md"));
            input[$field] = $value;
            f.check(input, true);
        });
    };
}
bad_payload!(non_string_event_denied, "hook_event_name", json!(42));
bad_payload!(relative_cwd_denied, "cwd", json!("doc/product"));
bad_payload!(non_object_input_denied, "tool_input", json!([]));
bad_payload!(missing_path_denied, "tool_input", json!({}));
bad_payload!(
    non_string_path_denied,
    "tool_input",
    json!({"file_path": 42})
);

case!(oversize_payload_denied, SessionKind::Authoring, f, {
    let mut input = payload("Write", &f.root.join("doc/product/x.md"));
    input["padding"] = json!("x".repeat(1024 * 1024));
    f.check(input, true);
});
case!(invalid_json_denied, SessionKind::Authoring, f, {
    assert_deny(f.run(b"{"));
});
case!(missing_guard_denied, SessionKind::Authoring, f, {
    fs::remove_file(f.session.guard())?;
    f.write(&f.root.join("doc/product/x.md"), true);
});
case!(invalid_guard_json_denied, SessionKind::Authoring, f, {
    fs::write(f.session.guard(), b"{")?;
    f.write(&f.root.join("doc/product/x.md"), true);
});
case!(
    post_tool_use_silent_even_without_guard,
    SessionKind::Authoring,
    f,
    {
        fs::remove_file(f.session.guard())?;
        f.check(json!({"hook_event_name": "PostToolUse"}), false);
    }
);

fn search(f: &Fixture, tool: &str, input: Value, denied: bool) {
    let call = json!({"hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input});
    f.check(call, denied);
}

case!(state_root_search_denied, SessionKind::Authoring, f, {
    let state = f.root.join(".loom/work");
    let history = state.join("terminal-history");
    fs::create_dir_all(&history)?;
    fs::write(history.join("log"), "x")?;
    for tool in ["Grep", "Glob"] {
        for root in [state.clone(), f.root.join(".loom"), state.join("signals")] {
            search(&f, tool, json!({"path": root, "pattern": "*"}), true);
        }
        let call = json!({"hook_event_name": "PreToolUse", "tool_name": tool,
            "tool_input": {"pattern": "*"}, "cwd": state});
        f.check(call, true);
    }
    search(&f, "Grep", json!({"path": history, "pattern": "x"}), true);
});
case!(
    resolved_state_root_search_denied,
    SessionKind::Authoring,
    f,
    {
        let elsewhere = f.root.parent().unwrap().join("state");
        fs::create_dir_all(elsewhere.join("work"))?;
        fs::remove_dir_all(f.root.join(".loom/work"))?;
        symlink(elsewhere.join("work"), f.root.join(".loom/work"))?;
        for tool in ["Grep", "Glob"] {
            let roots = [
                elsewhere.join("work"),
                elsewhere.clone(),
                f.root.join(".loom/work"),
                f.root.join(".loom"),
            ];
            for root in roots {
                search(&f, tool, json!({"path": root, "pattern": "*"}), true);
            }
        }
    }
);
case!(filtered_search_above_state_root_denied, pressure(), f, {
    for glob in [json!("*.md"), json!("*"), json!(".loom/**"), json!(["*"])] {
        let input = json!({"path": f.root, "pattern": "x", "glob": glob});
        search(&f, "Grep", input, true);
    }
    for pattern in ["../x/*", "{..,doc}/*"] {
        search(
            &f,
            "Glob",
            json!({"path": f.root, "pattern": pattern}),
            true,
        );
    }
});
case!(
    unfiltered_or_beside_state_root_search_deferred,
    SessionKind::Summary,
    f,
    {
        let doc = f.root.join("doc");
        search(
            &f,
            "Grep",
            json!({"path": doc, "pattern": "x", "glob": "*"}),
            false,
        );
        search(
            &f,
            "Grep",
            json!({"path": f.root, "pattern": "x", "glob": ""}),
            false,
        );
        search(&f, "Grep", json!({"path": f.root, "pattern": "x"}), false);
        search(
            &f,
            "Glob",
            json!({"path": f.root, "pattern": "**/*.md"}),
            false,
        );
    }
);

macro_rules! bad_policy {
    ($name:ident, $field:literal, $value:expr) => {
        case!($name, SessionKind::Authoring, f, {
            let mut policy: Value = serde_json::from_slice(&fs::read(f.session.guard())?)?;
            policy[$field] = $value;
            fs::write(f.session.guard(), serde_json::to_vec(&policy)?)?;
            f.write(&f.root.join("doc/product/x.md"), true);
        });
    };
}
bad_policy!(schema_two_denied, "schema", json!(2));
bad_policy!(unknown_policy_field_denied, "unknown", json!(true));
bad_policy!(
    different_session_dir_denied,
    "session_dir",
    json!("/another/session")
);
