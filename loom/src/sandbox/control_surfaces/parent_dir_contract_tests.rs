//! Contract `dotdot-or-codex-rule-is-propagated`: a permission rule whose
//! path climbs through `..` onto a control surface, on an absolute or `~/`
//! path and inside a Bash word alike, or that names `.codex`, is a rule on a
//! control surface.

use std::path::{Path, PathBuf};

use super::ControlSurfaces;

#[test]
fn a_parent_dir_or_codex_component_marks_a_rule() {
    let surfaces = ControlSurfaces::new(
        Path::new("/repo/.loom/work"),
        Some(Path::new("/run/user/1000/loom/scratch")),
        &[PathBuf::from("/opt/loom-hooks")],
        Some(Path::new("/home/op")),
    );

    let unmarked: Vec<&str> = [
        "Edit(~/x/../.claude.json)",
        "Bash(cp x ~/y/../.claude.json)",
        "Edit(.codex/config.toml)",
        "Edit(~/foo/../.codex/hooks.json)",
    ]
    .into_iter()
    .filter(|rule| !surfaces.names(rule))
    .collect();

    assert!(
        unmarked.is_empty(),
        "rules reaching a control surface were not marked: {unmarked:?}"
    );
    assert!(
        !surfaces.names("Edit(src/lib.rs)"),
        "an ordinary source path was marked as a control surface"
    );
}
