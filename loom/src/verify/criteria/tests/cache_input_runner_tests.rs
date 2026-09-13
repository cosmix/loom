//! End-to-end cache-input eligibility and stability tests.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::models::stage::{AcceptanceCriterion, Stage, TruthCheck};
use crate::verify::criteria::executor::OUTPUT_TRUNCATED_MARKER;
use crate::verify::criteria::{run_acceptance_with_config, CachePolicy, CriteriaConfig};

const TEST_IDENTITY: &[&str] = &["-c", "user.name=Test", "-c", "user.email=test@example.com"];

struct InputFixture {
    repo: TempDir,
    cache: TempDir,
    marker: PathBuf,
}

impl InputFixture {
    fn new(script_body: &str) -> Self {
        let repo = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        let marker = cache.path().join("executions.log");
        git(&["init", "-q"], repo.path());
        std::fs::create_dir_all(repo.path().join("scripts")).unwrap();
        write_probe(repo.path(), &marker, script_body);
        std::fs::write(repo.path().join("source.txt"), "source\n").unwrap();
        git(&["add", "scripts/probe.sh", "source.txt"], repo.path());
        git(&["commit", "-q", "-m", "fixture"], repo.path());
        Self {
            repo,
            cache,
            marker,
        }
    }

    fn config(&self) -> CriteriaConfig {
        CriteriaConfig::with_timeout(Duration::from_secs(2))
            .with_cache_dir(self.cache.path())
            .with_cache_policy(CachePolicy::Use)
    }

    fn executions(&self) -> usize {
        std::fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn cache_records(&self) -> usize {
        let cache_dir = self.cache.path().join("acceptance-cache");
        std::fs::read_dir(cache_dir)
            .map(|entries| entries.count())
            .unwrap_or_default()
    }
}

fn write_probe(repo: &Path, marker: &Path, body: &str) {
    let content = format!(
        "#!/bin/sh\nprintf 'run\\n' >> '{}'\n{body}\n",
        marker.display()
    );
    std::fs::write(repo.join("scripts/probe.sh"), content).unwrap();
}

fn git(args: &[&str], dir: &Path) {
    let mut full = TEST_IDENTITY.to_vec();
    full.extend_from_slice(args);
    let status = Command::new("git")
        .args(&full)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn extended(command: &str, required: &str) -> Stage {
    let mut stage = Stage::new("input-cache-test".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Extended(TruthCheck {
        command: command.to_string(),
        stdout_contains: vec![required.to_string()],
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: Some(0),
        description: None,
    }));
    stage
}

fn with_counter_setup(mut stage: Stage) -> Stage {
    stage.setup.push("sh scripts/probe.sh".to_string());
    stage
}

fn run(fixture: &InputFixture, stage: &Stage) -> crate::verify::criteria::AcceptanceResult {
    run_acceptance_with_config(stage, Some(fixture.repo.path()), &fixture.config()).unwrap()
}

#[test]
#[serial]
fn command_changing_its_tracked_input_never_publishes() {
    let fixture = InputFixture::new("printf x >> source.txt");
    let stage = extended("sh scripts/probe.sh", "");

    let first = run(&fixture, &stage);
    let second = run(&fixture, &stage);

    assert!(first.all_passed() && second.all_passed());
    assert!(!first.results()[0].cached && !second.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn truncated_command_output_is_not_cached_and_reexecutes() {
    let fixture = InputFixture::new("head -c 11000000 /dev/zero | tr '\\0' 'a'");
    let stage = extended("sh scripts/probe.sh", "a");

    let first = run(&fixture, &stage);
    assert_eq!(fixture.cache_records(), 0);
    let second = run(&fixture, &stage);

    assert!(first.all_passed() && second.all_passed());
    assert!(first.results()[0].stdout.contains(OUTPUT_TRUNCATED_MARKER));
    assert!(second.results()[0].stdout.contains(OUTPUT_TRUNCATED_MARKER));
    assert!(!first.results()[0].cached && !second.results()[0].cached);
    assert_eq!(fixture.cache_records(), 0);
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn untracked_content_hit_has_neighboring_byte_mutation_miss() {
    let fixture = InputFixture::new("cat input.dat");
    std::fs::write(fixture.repo.path().join("input.dat"), "alpha").unwrap();
    let stage = extended("sh scripts/probe.sh", "alpha");

    let first = run(&fixture, &stage);
    let hit = run(&fixture, &stage);
    std::fs::write(fixture.repo.path().join("input.dat"), "bravo").unwrap();
    let changed = run(&fixture, &stage);

    assert!(first.all_passed() && hit.all_passed());
    assert!(hit.results()[0].cached);
    assert!(!changed.all_passed() && !changed.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn ignored_and_external_inputs_bypass_and_reexecute_after_mutation() {
    let fixture = InputFixture::new(":");
    std::fs::write(fixture.repo.path().join(".gitignore"), "ignored.dat\n").unwrap();
    git(&["add", ".gitignore"], fixture.repo.path());
    git(&["commit", "-q", "-m", "ignore"], fixture.repo.path());
    let ignored = fixture.repo.path().join("ignored.dat");
    std::fs::write(&ignored, "alpha").unwrap();
    let ignored_stage = with_counter_setup(extended("cat ignored.dat", "alpha"));
    let first = run(&fixture, &ignored_stage);
    std::fs::write(&ignored, "bravo").unwrap();
    let changed = run(&fixture, &ignored_stage);
    assert!(first.all_passed() && !changed.all_passed());

    let external = fixture.cache.path().join("external.dat");
    std::fs::write(&external, "alpha").unwrap();
    let external_stage =
        with_counter_setup(extended(&format!("cat {}", external.display()), "alpha"));
    let external_first = run(&fixture, &external_stage);
    std::fs::write(&external, "bravo").unwrap();
    let external_changed = run(&fixture, &external_stage);

    assert!(external_first.all_passed() && !external_changed.all_passed());
    assert!(changed.results().iter().all(|result| !result.cached));
    assert!(external_changed
        .results()
        .iter()
        .all(|result| !result.cached));
    assert_eq!(fixture.executions(), 4);
}

#[cfg(unix)]
#[test]
#[serial]
fn tracked_source_symlink_change_bypasses_and_reexecutes() {
    use std::os::unix::fs::symlink;

    let fixture = InputFixture::new(":");
    std::fs::write(fixture.repo.path().join("alpha.dat"), "alpha").unwrap();
    std::fs::write(fixture.repo.path().join("bravo.dat"), "bravo").unwrap();
    symlink("alpha.dat", fixture.repo.path().join("source.link")).unwrap();
    git(
        &["add", "alpha.dat", "bravo.dat", "source.link"],
        fixture.repo.path(),
    );
    git(&["commit", "-q", "-m", "symlink"], fixture.repo.path());
    let stage = with_counter_setup(extended("cat source.link", "alpha"));
    let first = run(&fixture, &stage);
    std::fs::remove_file(fixture.repo.path().join("source.link")).unwrap();
    symlink("bravo.dat", fixture.repo.path().join("source.link")).unwrap();
    let changed = run(&fixture, &stage);

    assert!(first.all_passed() && !changed.all_passed());
    assert!(!first.results()[0].cached && !changed.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[cfg(unix)]
#[test]
#[serial]
fn unreadable_and_over_budget_inputs_bypass() {
    use std::os::unix::fs::PermissionsExt;

    let unreadable = InputFixture::new(":");
    let private = unreadable.repo.path().join("private.dat");
    std::fs::write(&private, "private").unwrap();
    let mut permissions = private.metadata().unwrap().permissions();
    permissions.set_mode(0o000);
    std::fs::set_permissions(&private, permissions).unwrap();
    let stage = with_counter_setup(extended("true", ""));
    assert!(run(&unreadable, &stage).all_passed());
    assert!(run(&unreadable, &stage).all_passed());
    assert_eq!(unreadable.executions(), 2);

    let oversized = InputFixture::new(":");
    let file = std::fs::File::create(oversized.repo.path().join("oversized.dat")).unwrap();
    file.set_len(65 * 1024 * 1024).unwrap();
    let stage = with_counter_setup(extended("true", ""));
    let first = run(&oversized, &stage);
    let second = run(&oversized, &stage);
    assert!(first.all_passed() && second.all_passed());
    assert!(!first.results()[0].cached && !second.results()[0].cached);
    assert_eq!(oversized.executions(), 2);
}

#[test]
#[serial]
fn legacy_and_corrupt_records_each_force_a_real_execution() {
    let fixture = InputFixture::new("printf valid");
    let stage = extended("sh scripts/probe.sh", "valid");
    let first = run(&fixture, &stage);
    let cache_dir = fixture.cache.path().join("acceptance-cache");
    let record = std::fs::read_dir(&cache_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(&record, "{\"command\":\"legacy\"}").unwrap();
    let second = run(&fixture, &stage);
    std::fs::write(record, "{\"version\":2").unwrap();
    let third = run(&fixture, &stage);

    assert!(first.all_passed() && second.all_passed() && third.all_passed());
    assert!(!first.results()[0].cached && !second.results()[0].cached);
    assert!(!third.results()[0].cached);
    assert_eq!(fixture.executions(), 3);
}

struct PathGuard {
    original: Option<std::ffi::OsString>,
}

impl PathGuard {
    fn set(path: &std::ffi::OsStr) -> Self {
        let original = std::env::var_os("PATH");
        std::env::set_var("PATH", path);
        Self { original }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[cfg(unix)]
#[test]
#[serial]
fn resolved_nested_executable_change_misses_after_neighboring_hit() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = InputFixture::new(":");
    let bin = TempDir::new().unwrap();
    let tool = bin.path().join("cache-probe-tool");
    write_tool(&tool, "first");
    let joined =
        std::env::join_paths([bin.path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    let _guard = PathGuard::set(&joined);
    let stage = extended("cache-probe-tool", "");
    let first = run(&fixture, &stage);
    let hit = run(&fixture, &stage);
    write_tool(&tool, "second");
    let changed = run(&fixture, &stage);

    assert!(first.all_passed() && hit.all_passed() && changed.all_passed());
    assert!(hit.results()[0].cached && !changed.results()[0].cached);
    assert_eq!(fixture.executions(), 2);

    fn write_tool(path: &Path, label: &str) {
        std::fs::write(
            path,
            format!("#!/bin/sh\n# {label}\nexec /bin/sh scripts/probe.sh\n"),
        )
        .unwrap();
        let mut permissions = path.metadata().unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}
