//! The host environment the cache reads: which names, what is dropped, and
//! how the Go settings reach each consumer.

use super::tests::{temp, var};
use super::{Consumer, StageCache};

/// `GOPROXY` and `GOFLAGS` the host sets are kept when credential-free;
/// `MAVEN_OPTS` is never read.
#[test]
fn credential_free_go_settings_are_kept_and_maven_opts_is_never_read() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let vars = [
        ("HOME", home.as_os_str().to_owned()),
        ("GOPROXY", "https://proxy.example,direct".into()),
        ("GOFLAGS", "-mod=mod".into()),
        ("MAVEN_OPTS", "-Dhttp.proxyPassword=secret".into()),
    ];
    let host = super::host_env_from(vars.map(|(k, v)| (k.into(), v)));
    assert!(!host.contains_key("MAVEN_OPTS"), "{host:?}");
    let local = format!("-Dmaven.repo.local={}", cache.root().join("m2").display());
    for consumer in [Consumer::Session, Consumer::Worker, Consumer::HostJob] {
        let env = cache.environment(consumer, &tree, &host, None);
        assert_eq!(var(&env, "MAVEN_OPTS"), Some(local.clone()), "{consumer:?}");
        let (goflags, proxy) = match consumer {
            Consumer::HostJob => ("-mod=mod -count=1", "off"),
            _ => ("-mod=mod", "https://proxy.example,direct"),
        };
        assert_eq!(
            var(&env, "GOFLAGS").as_deref(),
            Some(goflags),
            "{consumer:?}"
        );
        assert_eq!(var(&env, "GOPROXY").as_deref(), Some(proxy), "{consumer:?}");
        let leaked = env.values().any(|v| v.to_string_lossy().contains("secret"));
        assert!(!leaked, "{consumer:?}: {env:?}");
    }
}

/// A `GOPROXY` with URL userinfo in any entry is a credential: dropped, so the
/// default chain applies. Userinfo-free `@` elsewhere in a URL is kept.
#[test]
fn a_goproxy_with_userinfo_is_dropped_whichever_entry_carries_it() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let proxy_for = |goproxy: &str| {
        let vars = [("HOME", home.as_os_str()), ("GOPROXY", goproxy.as_ref())];
        let host = super::host_env_from(vars.map(|(k, v)| (k.into(), v.to_owned())));
        let env = cache.environment(Consumer::Session, &tree, &host, None);
        var(&env, "GOPROXY")
    };
    let default = Some("https://proxy.golang.org,direct".to_string());
    for credential in [
        "https://user:secret@proxy.example",
        "https://token@proxy.example/path",
        "https://proxy.example,https://user:secret@other.example|direct",
        "user:token@proxy.corp.example,direct",
        "direct,token@proxy.corp.example/path",
    ] {
        assert_eq!(proxy_for(credential), default, "{credential}");
    }
    let kept = "https://proxy.example/a@b,direct";
    assert_eq!(proxy_for(kept).as_deref(), Some(kept));
}

/// The process environment the consumers read seeds from keeps the seed
/// locations and credential-free Go settings, and drops a userinfo `GOPROXY`
/// and `MAVEN_OPTS`.
#[test]
#[serial_test::serial]
fn the_process_host_environment_keeps_locations_and_credential_free_settings() {
    let names = [
        "GOPROXY",
        "GOFLAGS",
        "MAVEN_OPTS",
        "GOMODCACHE",
        "GRADLE_USER_HOME",
    ];
    let saved: Vec<_> = names.map(|name| (name, std::env::var_os(name))).into();
    let read = |goproxy: &str| {
        std::env::set_var("GOPROXY", goproxy);
        std::env::set_var("GOFLAGS", "-mod=mod");
        std::env::set_var("MAVEN_OPTS", "-Dhttp.proxyPassword=secret");
        std::env::set_var("GOMODCACHE", "/nonexistent/go-mod");
        std::env::set_var("GRADLE_USER_HOME", "/nonexistent/gradle");
        super::host_env_from_process()
    };
    let free = read("https://proxy.example");
    let credential = read("https://user:secret@proxy.example");
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    assert_eq!(
        var(&free, "GOPROXY").as_deref(),
        Some("https://proxy.example")
    );
    assert_eq!(var(&free, "GOFLAGS").as_deref(), Some("-mod=mod"));
    assert!(!credential.contains_key("GOPROXY"), "{credential:?}");
    assert_eq!(var(&credential, "GOFLAGS").as_deref(), Some("-mod=mod"));
    for host in [&free, &credential] {
        assert!(!host.contains_key("MAVEN_OPTS"), "{host:?}");
        let modcache = var(host, "GOMODCACHE");
        assert_eq!(modcache.as_deref(), Some("/nonexistent/go-mod"));
        let gradle = var(host, "GRADLE_USER_HOME");
        assert_eq!(gradle.as_deref(), Some("/nonexistent/gradle"));
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let env = cache.environment(Consumer::Session, &tree, &credential, None);
    let proxy = var(&env, "GOPROXY").unwrap();
    assert_eq!(proxy, "https://proxy.golang.org,direct");
}

/// A seed reached through a symlink is named by its canonical path, the one its
/// read root grants.
#[test]
fn a_symlinked_seed_is_named_by_its_canonical_path() {
    let ((_r, repo), (_h, home), (_g, real), (_t, tree)) = (temp(), temp(), temp(), temp());
    std::fs::create_dir_all(real.join("cache/download")).unwrap();
    let link = home.join("gomod-link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let vars = [
        ("HOME", home.as_os_str().to_owned()),
        ("GOMODCACHE", link.as_os_str().to_owned()),
    ];
    let host = super::host_env_from(vars.map(|(k, v)| (k.into(), v)));
    let env = cache.environment(Consumer::HostJob, &tree, &host, None);
    let seed = real.join("cache/download");
    let expected = format!("file://{},off", seed.display());
    assert_eq!(var(&env, "GOPROXY"), Some(expected));
    assert_eq!(super::seed_read_roots(&host, &[]), vec![seed]);
}
