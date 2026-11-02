use super::*;

#[test]
fn host_caches_are_selected_and_commands_do_not_share_writable_state() {
    let fixture = tempfile::tempdir().unwrap();
    let tree = fixture.path().join("tree");
    let uv = fixture.path().join(".cache/uv");
    let bun = fixture.path().join(".bun/install/cache");
    for path in [&tree, &uv, &bun] {
        std::fs::create_dir_all(path).unwrap();
    }
    std::fs::write(uv.join("sentinel"), "host").unwrap();
    let mut command = Command::new("/bin/true");
    command.env("HOME", fixture.path());
    let lowers = cache_lowers(&command);
    assert_eq!(lowers, vec![("uv-cache", uv.clone()), ("bun-cache", bun)]);
    let first = CargoScratch::prepare(&command, &tree, true).unwrap();
    let second = CargoScratch::prepare(&command, &tree, true).unwrap();
    first.populate().unwrap();
    second.populate().unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(
        std::fs::read_to_string(uv.join("sentinel")).unwrap(),
        "host"
    );
}

#[test]
fn overlay_keeps_host_cache_unchanged_across_concurrent_commands() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("host-cache");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("package"), "warm").unwrap();
    if !overlay_supported(fixture.path(), "probe", &source) {
        eprintln!("overlay cache test skipped: bubblewrap overlay unavailable");
        return;
    }
    std::thread::scope(|scope| {
        for name in ["first", "second"] {
            let source = &source;
            let root = fixture.path();
            scope.spawn(move || {
                assert!(overlay_supported(root, name, source));
                let overlay = root.join(format!("overlay-{name}"));
                let target = root.join(name);
                let status = Command::new("bwrap")
                    .args(["--unshare-all", "--ro-bind", "/", "/", "--overlay-src"])
                    .arg(source)
                    .arg("--overlay")
                    .arg(overlay.join("upper"))
                    .arg(overlay.join("work"))
                    .arg(&target)
                    .args([
                        "--",
                        "/bin/sh",
                        "-c",
                        "test \"$(cat \"$1/package\")\" = warm && printf changed > \"$1/package\"",
                        "sh",
                    ])
                    .arg(&target)
                    .status()
                    .unwrap();
                restore_overlay_work_permissions(&overlay.join("work"));
                assert!(status.success());
            });
        }
    });
    assert_eq!(
        std::fs::read_to_string(source.join("package")).unwrap(),
        "warm"
    );
}

#[test]
fn absent_host_caches_do_not_enable_an_overlay() {
    let fixture = tempfile::tempdir().unwrap();
    let mut command = Command::new("/bin/true");
    command.env("HOME", fixture.path());
    assert!(cache_lowers(&command).is_empty());
}
