//! The tick step and a static refusal returned to the writer: the record
//! waits while the writer works on it, is filed once the writer goes quiet,
//! and a filed one (`returned = 0`) is filed at once. Uses the fixture of
//! `freeze_refusal_tests.rs`.

use serial_test::serial;

use super::tests::{fixture, Fixture};
use super::*;
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};
use crate::verify::contracts::refusal::{record_static_refusal, JobRefusalOrigin, StaticRefusal};

const STAGE: &str = "test-stage";

/// Record the writer's static refusal of the job submitted `order`th.
fn refuse_statically(fx: &Fixture, order: u64) -> StaticRefusal {
    let origin = JobRefusalOrigin {
        job_id: format!("{order:032x}"),
        submission_order: order,
    };
    let problems = vec![format!("format: tests/a.rs is not formatted (job {order})")];
    record_static_refusal(&fx.work, STAGE, &fx.writer.id, origin, &problems).unwrap()
}

fn returned(fx: &Fixture) -> u32 {
    daemon_refusal(&fx.work, STAGE)
        .unwrap()
        .expect("the refusal is still on record")
        .returned
}

#[test]
#[serial]
fn a_returned_refusal_waits_while_the_writer_is_working() {
    let mut fx = fixture();
    assert_eq!(
        refuse_statically(&fx, 1),
        StaticRefusal::Returned { count: 1 }
    );

    fx.tick();

    fx.assert_nothing_filed();
    assert_eq!(returned(&fx), 1, "the record waits for the writer");
}

#[test]
#[serial]
fn a_returned_refusal_waits_while_the_writer_still_heartbeats() {
    let mut fx = fixture();
    refuse_statically(&fx, 1);
    let mut heartbeat = Heartbeat::new(STAGE.to_string(), fx.writer.id.clone());
    heartbeat.timestamp = chrono::Utc::now() + chrono::Duration::seconds(300);
    write_heartbeat(&fx.work, &heartbeat).unwrap();

    fx.tick_after_quiet();

    fx.assert_nothing_filed();
    assert_eq!(returned(&fx), 1);
}

#[test]
#[serial]
fn a_returned_refusal_is_filed_once_the_writer_is_quiet() {
    let mut fx = fixture();
    refuse_statically(&fx, 1);
    refuse_statically(&fx, 2);

    fx.tick_after_quiet();

    let request = fx.assert_filed_once();
    assert!(
        request.contains("tests/a.rs is not formatted (job 2)"),
        "{request}"
    );
    assert_eq!(fx.stage_tally(), 1);
}

#[test]
#[serial]
fn the_third_static_refusal_is_filed_at_once() {
    let mut fx = fixture();
    refuse_statically(&fx, 1);
    refuse_statically(&fx, 2);
    assert_eq!(refuse_statically(&fx, 3), StaticRefusal::Filed);

    fx.tick();

    let request = fx.assert_filed_once();
    assert!(
        request.contains("tests/a.rs is not formatted (job 3)"),
        "{request}"
    );
}

impl Fixture {
    /// The stage's count of filed freeze-refusal disputes.
    fn stage_tally(&self) -> u32 {
        crate::verify::transitions::load_stage(STAGE, &self.work)
            .unwrap()
            .tally
            .freeze_refusal_disputes
    }
}
