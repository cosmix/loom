//! Contract of the runtime directory an in-crate `PlanContext` resolves with `LOOM_HOME` unset.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serial_test::serial;

use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::prereq::ReadOnly;
use crate::planning::watch::OwnWrites;

#[test]
#[serial]
fn in_crate_context_stays_out_of_home() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("doc")).unwrap();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let old_home = std::env::var_os("LOOM_HOME");
    std::env::remove_var("LOOM_HOME");
    let context = PlanContext::new(
        root,
        ReadOnly::from_problems(Vec::new()),
        BindInfo::new(local, "test"),
        OwnWrites::default(),
    );
    if let Some(value) = old_home {
        std::env::set_var("LOOM_HOME", value);
    }
    let context = context.unwrap();
    let real = dirs::home_dir().unwrap().join(".loom");
    let dir = &context.runtime().dir;
    assert!(
        !dir.starts_with(&real),
        "{} lies under the operator's {}",
        dir.display(),
        real.display()
    );
}
