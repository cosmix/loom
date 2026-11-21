//! Contract: a macOS host qualifies for native workers exactly when its three
//! probes pass, asked in order, and the first failing probe stops the rest.
use std::cell::RefCell;

use super::probe::{qualify_macos_from, MacHostProbes};

const PROBES: [&str; 3] = ["sandbox_exec", "payload_boundary", "codex_home_honoured"];

/// A host whose probe `failing` (an index into [`PROBES`]) fails, recording
/// every probe it is asked.
struct FakeMac {
    failing: Option<usize>,
    asked: RefCell<Vec<&'static str>>,
}

impl FakeMac {
    fn new(failing: Option<usize>) -> Self {
        Self {
            failing,
            asked: RefCell::new(Vec::new()),
        }
    }

    fn answer(&self, index: usize) -> Result<(), String> {
        self.asked.borrow_mut().push(PROBES[index]);
        match self.failing {
            Some(failing) if failing == index => Err(reason(index)),
            _ => Ok(()),
        }
    }
}

fn reason(index: usize) -> String {
    format!("fake refusal from {}", PROBES[index])
}

impl MacHostProbes for FakeMac {
    fn sandbox_exec(&self) -> Result<(), String> {
        self.answer(0)
    }

    fn payload_boundary(&self) -> Result<(), String> {
        self.answer(1)
    }

    fn codex_home_honoured(&self) -> Result<(), String> {
        self.answer(2)
    }
}

#[test]
fn a_macos_host_whose_probes_pass_qualifies() {
    let host = FakeMac::new(None);
    assert_eq!(qualify_macos_from(&host), Ok(()));
    assert_eq!(
        *host.asked.borrow(),
        PROBES.to_vec(),
        "every probe, in order"
    );

    for failing in 0..PROBES.len() {
        let host = FakeMac::new(Some(failing));
        let refusal =
            qualify_macos_from(&host).expect_err("a host with a failing probe must not qualify");
        assert!(
            refusal.contains(&reason(failing)),
            "the refusal names the failing probe's reason: {refusal}"
        );
        assert_eq!(
            *host.asked.borrow(),
            PROBES[..=failing].to_vec(),
            "probes run in order and stop at the first failure"
        );
    }
}
