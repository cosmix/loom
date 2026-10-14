//! The instruction a guarded-path refusal prints, followed to the letter:
//! the command is lifted from the refusal text and run, then the commit is
//! made again.

use super::*;

/// The committed content of `path` in commit `id`, `None` where absent.
fn committed(root: &Path, id: &str, path: &str) -> Option<String> {
    let listed = git_in(root, &["ls-tree", "--name-only", id, "--", path]);
    (!listed.is_empty()).then(|| git_in(root, &["show", &format!("{id}:{path}")]))
}

/// Refuse `doc/specs/conf.txt` authored by the resolver, follow the refusal's
/// command for `source` and commit; the entry that lands.
fn conflicted_path_resolved_by(source: &str) -> Option<String> {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, Some("mine\n"), None);
    let refusal = guarded_refusal(root);
    assert!(refusal.contains("'doc/specs/conf.txt'"), "{refusal}");

    follow_printed(root, &refusal, Some(source), "doc/specs/conf.txt");
    let id = commit_guarding(root, &specs()).unwrap();

    committed(root, &id, "doc/specs/conf.txt")
}

#[test]
fn a_conflicted_path_with_authored_content_is_put_right_by_the_head_command() {
    assert_eq!(conflicted_path_resolved_by("HEAD").as_deref(), Some("ours"));
}

#[test]
fn a_conflicted_path_with_authored_content_is_put_right_by_the_merge_head_command() {
    assert_eq!(
        conflicted_path_resolved_by("MERGE_HEAD").as_deref(),
        Some("theirs")
    );
}

#[test]
fn a_modify_delete_conflict_authored_on_a_guarded_path_is_put_right_by_either_command() {
    // `main` deleted `gone.txt` and `loom/s1` edited it.
    for (source, landed) in [("HEAD", Some("ours")), ("MERGE_HEAD", None)] {
        let temp = protected_merge();
        let root = temp.path();
        resolve(root, None, None);
        stage(root, "doc/specs/gone.txt", "mine\n");
        let refusal = guarded_refusal(root);
        assert!(refusal.contains("'doc/specs/gone.txt'"), "{refusal}");

        follow_printed(root, &refusal, Some(source), "doc/specs/gone.txt");
        let id = commit_guarding(root, &specs()).unwrap();

        assert_eq!(
            committed(root, &id, "doc/specs/gone.txt").as_deref(),
            landed,
            "{source}"
        );
        assert_eq!(root.join("doc/specs/gone.txt").exists(), landed.is_some());
    }
}

/// `loom/s1` merging `main`, `--no-commit`: `special` is edited in different
/// places on both sides and merges cleanly, `bystander` is edited in the same
/// place and conflicts. A pathspec for `special` that is not literal matches
/// `bystander` too.
fn glob_named_merge(special: &str, bystander: &str) -> TempDir {
    let temp = repo();
    let root = temp.path();
    let write = |auto: &str, plain: &str| {
        stage(root, special, auto);
        stage(root, bystander, plain);
    };
    write(AUTO_BASE, "base\n");
    git_in(root, &["commit", "-m", "base"]);
    git_in(root, &["branch", "-f", "main", "HEAD"]);
    write(AUTO_OURS, "ours\n");
    git_in(root, &["commit", "-m", "ours"]);
    git_in(root, &["checkout", "main"]);
    write(AUTO_THEIRS, "theirs\n");
    git_in(root, &["commit", "-m", "theirs"]);
    git_in(root, &["checkout", "loom/s1"]);
    begin_merge(root);
    temp
}

/// Edit `special` past git's merge result and resolve `bystander`, which
/// refuses the commit; follow the refusal's command for `special`, commit, and
/// check that `special` holds the merge result and `bystander` what it held.
/// The directory is returned for the checks that look past the commit.
fn restored_by_the_printed_command(special: &str, bystander: &str) -> (TempDir, String) {
    let temp = glob_named_merge(special, bystander);
    let root = temp.path();
    stage(root, bystander, "ours\n");
    stage(root, special, &format!("{AUTO_MERGED}extra\n"));
    let refusal = guarded_refusal(root);
    assert_eq!(refusal.lines().count(), 1, "{refusal}");

    follow_printed(root, &refusal, None, special);
    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        committed(root, &id, special).as_deref(),
        Some(AUTO_MERGED.trim_end()),
        "{special}"
    );
    assert_eq!(
        committed(root, &id, bystander).as_deref(),
        Some("ours"),
        "{bystander}"
    );
    (temp, refusal)
}

#[test]
fn the_command_restores_a_guarded_path_named_with_glob_characters_and_nothing_else() {
    for (special, bystander) in [
        ("doc/specs/w[1].txt", "doc/specs/w1.txt"),
        ("doc/specs/s*.txt", "doc/specs/sx.txt"),
        ("doc/specs/q?.txt", "doc/specs/qz.txt"),
    ] {
        let (_temp, refusal) = restored_by_the_printed_command(special, bystander);

        assert!(refusal.contains(&format!("'{special}'")), "{refusal}");
        assert!(!refusal.contains(bystander), "{refusal}");
    }
}

#[test]
fn a_command_substitution_in_a_guarded_name_restores_it_and_runs_nothing() {
    let (temp, refusal) =
        restored_by_the_printed_command("doc/specs/$(touch pwned).md", "doc/specs/plain.txt");

    assert!(
        refusal.contains("'doc/specs/$(touch pwned).md'"),
        "{refusal}"
    );
    assert!(!temp.path().join("pwned").exists());
}

#[test]
fn a_quote_a_space_and_a_backtick_in_a_guarded_name_restore_it() {
    let (_temp, refusal) =
        restored_by_the_printed_command("doc/specs/it's a `b`.md", "doc/specs/plain.txt");

    assert!(
        refusal.contains(r"'doc/specs/it'\''s a `b`.md'"),
        "{refusal}"
    );
}

#[test]
fn a_newline_and_a_tab_in_a_guarded_name_are_written_ansi_c_and_restore_it() {
    let (_temp, refusal) =
        restored_by_the_printed_command("doc/specs/a\nb\tc.md", "doc/specs/plain.txt");

    assert!(refusal.contains(r"$'doc/specs/a\nb\tc.md'"), "{refusal}");
    assert!(!refusal.contains(['\n', '\t']), "{refusal}");
}

#[test]
fn a_combining_mark_in_a_guarded_name_is_printed_as_it_is_and_restores_it() {
    let (_temp, refusal) =
        restored_by_the_printed_command("doc/specs/cafe\u{301}.md", "doc/specs/plain.txt");

    assert!(refusal.contains("'doc/specs/cafe\u{301}.md'"), "{refusal}");
}
