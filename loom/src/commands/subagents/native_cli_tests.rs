use std::{fs, io::Write, path::PathBuf};

use super::{
    native_args::*,
    native_control,
    native_start::{self, Site},
    native_view, SubagentsCommand,
};
use crate::{
    models::session::SessionType,
    native_worker::{ids::*, index, model::*, store},
    relay::{
        emit::{
            test_support::{context_for, RelayFixture},
            RelayMode, RelaySink,
        },
        RelayLine, RequestKind, Ticket, WorkerControlPayload,
    },
    utils::sha256_hex,
};

const INV: &str = "inv-0123456789abcdef0123456789abcdef";

#[derive(Default)]
struct VecSink {
    out: Vec<u8>,
    err: Vec<u8>,
}
impl RelaySink for VecSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.out
    }
    fn stderr(&mut self) -> &mut dyn Write {
        &mut self.err
    }
}

struct Harness {
    fixture: RelayFixture,
    site: Site,
    native_backend: bool,
    session: bool,
    out: Vec<u8>,
    err: Vec<u8>,
    sink: VecSink,
}

impl Harness {
    fn new() -> Self {
        let fixture = context_for(SessionType::Stage);
        let site = Site {
            mode: RelayMode::Relay(fixture.context.clone()),
            cwd: fixture.cwd.clone(),
        };
        fs::write(site.cwd.join("brief.md"), "task\n").unwrap();
        Self {
            fixture,
            site,
            native_backend: true,
            session: true,
            out: vec![],
            err: vec![],
            sink: VecSink::default(),
        }
    }
    fn root(&self) -> PathBuf {
        self.fixture.context.work_dir.clone().unwrap()
    }
    fn start_args(&self) -> StartArgs {
        StartArgs {
            provider: "codex".into(),
            unit: "unit".into(),
            brief: self.site.cwd.join("brief.md"),
            model: "gpt-6.1-sol".into(),
            effort: "high".into(),
        }
    }
    fn run(&mut self, command: SubagentsCommand) -> i32 {
        let root = self.root();
        let (out, err, sink) = (&mut self.out, &mut self.err, &mut self.sink);
        match command {
            SubagentsCommand::Start(a) => {
                native_start::start_in(a, &self.site, self.native_backend, err, sink)
            }
            SubagentsCommand::Message(a) => native_start::message_in(a, &self.site, err, sink),
            SubagentsCommand::Cancel(a) => native_control::cancel_in(
                a,
                self.session.then_some(&self.site),
                &root,
                out,
                err,
                sink,
            ),
            SubagentsCommand::Release(a) => {
                native_control::release_in(a, self.session, &root, out, err)
            }
            SubagentsCommand::Status(a) => {
                native_view::status_in(a, Some("stage-a"), &root, out, err)
            }
            SubagentsCommand::Result(a) => {
                native_view::result_in(a, Some("stage-a"), &root, out, err)
            }
            _ => panic!("unexpected command"),
        }
        .unwrap()
    }
    fn refused(&self, flag: &str) {
        assert!(String::from_utf8_lossy(&self.err).contains(flag));
        assert_eq!(
            fs::read_dir(&self.fixture.context.scratch_dir)
                .unwrap()
                .count(),
            0
        );
        assert!(self.out.is_empty() && self.sink.out.is_empty());
    }
    fn payload(&self) -> WorkerControlPayload {
        let stdout = String::from_utf8_lossy(&self.sink.out);
        let line = RelayLine::parse(stdout.lines().last().unwrap()).unwrap();
        assert_eq!(line.kind, RequestKind::WorkerControl);
        let files: Vec<_> = fs::read_dir(&self.fixture.context.scratch_dir)
            .unwrap()
            .collect();
        assert_eq!(files.len(), 1);
        let ticket = Ticket::decode(&fs::read(files[0].as_ref().unwrap().path()).unwrap()).unwrap();
        serde_json::from_value(ticket.payload).unwrap()
    }
    fn record(&self) -> PathBuf {
        let manifest = NativeManifest {
            v: 1,
            stage_id: "stage-a".into(),
            unit_id: "unit".into(),
            invocation_id: INV.into(),
            owner_session_id: "session-1".into(),
            workspace_root: self.site.cwd.clone(),
            brief_path: self.site.cwd.join("brief.md"),
            brief_sha256: sha256_hex(b"task\n"),
            model: "gpt-6.1-sol".into(),
            effort: "high".into(),
            deadline_secs: NATIVE_DEADLINE_SECS,
            created_at: chrono::Utc::now(),
        };
        store::create_invocation(&self.root(), &manifest, b"task\n").unwrap()
    }
}

fn message(text: Option<String>, file: Option<PathBuf>) -> SubagentsCommand {
    SubagentsCommand::Message(MessageArgs {
        worker: "codex:unit".into(),
        invocation: INV.into(),
        delivery: DeliveryArg::Steer,
        message_id: "message-1".into(),
        text,
        file,
    })
}
fn cancel(stage: Option<&str>) -> SubagentsCommand {
    SubagentsCommand::Cancel(CancelArgs {
        worker: "codex:unit".into(),
        invocation: stage.is_none().then(|| INV.into()),
        stage: stage.map(str::to_owned),
        reason: " stop now ".into(),
    })
}
fn release() -> SubagentsCommand {
    SubagentsCommand::Release(ReleaseArgs {
        stage: "stage-a".into(),
        invocation: INV.into(),
    })
}
fn view(result: bool, full: bool, json: bool) -> SubagentsCommand {
    if result {
        SubagentsCommand::Result(ResultArgs {
            worker: "codex:unit".into(),
            stage: None,
            full,
            json,
        })
    } else {
        SubagentsCommand::Status(StatusArgs {
            worker: "codex:unit".into(),
            stage: None,
            json,
        })
    }
}

#[test]
fn start_writes_one_ticket_and_the_relay_line_is_last() {
    let mut h = Harness::new();
    assert_eq!(h.run(SubagentsCommand::Start(h.start_args())), 0);
    assert!(h.out.is_empty());
    assert!(
        matches!(h.payload(), WorkerControlPayload::Start { brief_sha256, .. }
        if brief_sha256 == sha256_hex(b"task\n"))
    );
    assert!(String::from_utf8_lossy(&h.err).contains("--wait 90"));
}

macro_rules! start_refusal {
    ($name:ident, $flag:literal, |$h:ident, $a:ident| $change:block) => {
        #[test]
        fn $name() {
            let mut $h = Harness::new();
            let mut $a = $h.start_args();
            $change
            assert_eq!($h.run(SubagentsCommand::Start($a)), EXIT_REFUSED);
            $h.refused($flag);
        }
    };
}
start_refusal!(invalid_unit_id, "--unit-id", |h, a| {
    a.unit = "../bad".into();
});
start_refusal!(unknown_model, "--model", |h, a| {
    a.model = "unknown".into();
});
start_refusal!(unknown_effort, "--effort", |h, a| {
    a.effort = "unknown".into();
});
start_refusal!(legacy_backend, "codex.backend", |h, a| {
    h.native_backend = false;
    a.unit = "unit".into();
});
start_refusal!(relative_brief, "--brief", |h, a| {
    a.brief = "brief.md".into();
});
start_refusal!(missing_brief, "--brief", |h, a| {
    a.brief = h.site.cwd.join("missing");
});
start_refusal!(symlinked_brief, "--brief", |h, a| {
    let target = a.brief.clone();
    a.brief = h.site.cwd.join("link");
    std::os::unix::fs::symlink(target, &a.brief).unwrap();
});
start_refusal!(oversized_brief, "--brief", |h, a| {
    fs::File::create(&a.brief)
        .unwrap()
        .set_len(MAX_BRIEF_BYTES + 1)
        .unwrap();
    a.unit = "unit".into();
});
start_refusal!(non_utf8_brief, "--brief", |h, a| {
    fs::write(&a.brief, [0xff]).unwrap();
    a.unit = "unit".into();
});
start_refusal!(operator_mode, "stage session", |h, a| {
    h.site.mode = RelayMode::Operator;
    a.unit = "unit".into();
});
start_refusal!(legacy_mode, "LOOM_SCRATCH_DIR", |h, a| {
    h.site.mode = RelayMode::Legacy;
    a.unit = "unit".into();
});

#[test]
fn message_text_and_file_forms() {
    for file in [false, true] {
        let mut h = Harness::new();
        let path = h.site.cwd.join("message");
        fs::write(&path, "hello").unwrap();
        assert_eq!(
            h.run(message(
                (!file).then(|| "hello".into()),
                file.then_some(path)
            )),
            0
        );
        assert!(
            matches!(h.payload(), WorkerControlPayload::Message { text, delivery: Delivery::Steer, .. }
            if text == "hello")
        );
    }
}
#[test]
fn message_over_the_byte_limit_is_refused() {
    for file in [false, true] {
        let mut h = Harness::new();
        let path = h.site.cwd.join("message");
        let text = "é".repeat(MAX_MESSAGE_BYTES / 2 + 1);
        fs::write(&path, &text).unwrap();
        assert_eq!(
            h.run(message((!file).then_some(text), file.then_some(path))),
            3
        );
        h.refused(if file { "--file" } else { "--text" });
    }
}
#[test]
fn message_refuses_a_bad_invocation_id() {
    let mut h = Harness::new();
    let SubagentsCommand::Message(mut args) = message(Some("hi".into()), None) else {
        unreachable!()
    };
    args.invocation = "bad".into();
    assert_eq!(h.run(SubagentsCommand::Message(args)), 3);
    h.refused("--invocation");
}
#[test]
fn relay_context_refuses_before_writing_a_ticket() {
    let mut h = Harness::new();
    let RelayMode::Relay(context) = &mut h.site.mode else {
        unreachable!()
    };
    context.session_type = Some("invalid-session-type".into());
    assert_eq!(h.run(SubagentsCommand::Start(h.start_args())), 3);
    h.refused("session");
}

#[test]
fn failed_wait_hint_does_not_fail_an_emitted_request() {
    let mut h = Harness::new();
    let mut err: &mut [u8] = &mut [];
    assert_eq!(
        native_start::start_in(h.start_args(), &h.site, true, &mut err, &mut h.sink).unwrap(),
        0
    );
    h.payload();
}

#[path = "native_cli_control_tests.rs"]
mod control_tests;
#[path = "native_cli_parse_tests.rs"]
mod parse_tests;
#[path = "native_cli_view_tests.rs"]
mod view_tests;
