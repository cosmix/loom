use super::*;
use crate::models::stage::IssueDisposition;
use std::time::Duration;
use tempfile::TempDir;

const STAGE: &str = "server";

fn work_dir() -> (TempDir, WorkDir) {
    let temp = TempDir::new().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    (temp, work)
}

fn stage() -> Stage {
    Stage {
        id: STAGE.to_string(),
        ..Stage::default()
    }
}

/// Write dispute `id`'s request with `reason`, and each named record.
fn dispute(work: &WorkDir, id: u32, reason: &str, records: &[(&str, &str)]) -> PathBuf {
    let dir = work.disputes_dir().join(STAGE).join(id.to_string());
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("request.md"),
        format!("---\nkind: criterion\nreason: {reason}\n---\n"),
    )
    .unwrap();
    for (name, content) in records {
        fs::write(dir.join(name), content).unwrap();
    }
    dir
}

fn set_modified(path: &Path, at: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

const ACCEPT: &str = "---\nverdict:\n  verdict: accept\n  reasoning: fair\n\
                      created_at: \"2020-01-02T00:05:00Z\"\n---\n";

#[test]
fn a_verdict_not_yet_applied_is_not_an_ended_round() {
    let (_temp, work) = work_dir();
    dispute(&work, 1, "pending", &[("verdict.md", ACCEPT)]);
    assert!(adjudication_rounds(&stage(), &work).is_empty());

    dispute(&work, 1, "pending", &[("applied.marker", "")]);
    let rounds = adjudication_rounds(&stage(), &work);
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].issue.ended_by, "adjudicator verdict: accept");
    assert_eq!(
        rounds[0].issue.ended_at.to_rfc3339(),
        "2020-01-02T00:05:00+00:00"
    );
    assert_eq!(rounds[0].reason_lines, ["pending"]);
}

#[test]
fn an_applied_verdict_without_a_parseable_created_at_ends_when_it_was_written() {
    let (_temp, work) = work_dir();
    let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let dir = dispute(
        &work,
        1,
        "x",
        &[
            (
                "verdict.md",
                "---\nverdict:\n  verdict: reject\ncreated_at: soon\n---\n",
            ),
            ("applied.marker", ""),
        ],
    );
    set_modified(&dir.join("verdict.md"), written);

    let rounds = adjudication_rounds(&stage(), &work);

    assert_eq!(rounds.len(), 1, "a verdict is never dropped for its date");
    assert_eq!(rounds[0].issue.ended_by, "adjudicator verdict: reject");
    assert_eq!(rounds[0].issue.ended_at, DateTime::<Utc>::from(written));
}

#[test]
fn a_closed_dispute_with_an_unapplied_verdict_says_so() {
    let (_temp, work) = work_dir();
    dispute(
        &work,
        1,
        "x",
        &[("verdict.md", ACCEPT), (CLOSED_MARKER, "")],
    );

    let rounds = adjudication_rounds(&stage(), &work);

    assert_eq!(
        rounds[0].issue.ended_by,
        "closed before its verdict was applied"
    );
}

#[test]
fn an_unchanged_dispute_is_parsed_once_and_a_changed_stamp_parses_it_again() {
    let (_temp, work) = work_dir();
    let dir = dispute(&work, 1, "first", &[(CLOSED_MARKER, "")]);
    let reasons = || -> Vec<String> {
        adjudication_rounds(&stage(), &work)
            .into_iter()
            .flat_map(|round| round.reason_lines)
            .collect()
    };
    assert_eq!(reasons(), ["first"]);

    // A rewritten request under unchanged end markers is not read again.
    fs::write(
        dir.join("request.md"),
        "---\nkind: criterion\nreason: second\n---\n",
    )
    .unwrap();
    assert_eq!(reasons(), ["first"], "the cached round is reused");

    let marker = dir.join(CLOSED_MARKER);
    let later = fs::metadata(&marker).unwrap().modified().unwrap() + Duration::from_secs(60);
    set_modified(&marker, later);
    assert_eq!(reasons(), ["second"], "a new stamp re-reads the dispute");
}

#[test]
fn worktree_state_root_symlink_keeps_ended_rounds_readable() {
    let real = TempDir::new().unwrap();
    let alias = TempDir::new().unwrap();
    let loom_dir = alias.path().join(".loom");
    fs::create_dir(&loom_dir).unwrap();
    std::os::unix::fs::symlink(real.path(), loom_dir.join("work")).unwrap();
    let work = WorkDir::new(loom_dir.join("work")).unwrap();
    dispute(&work, 1, "recorded", &[(CLOSED_MARKER, "")]);

    let rounds = adjudication_rounds(&stage(), &work);
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].reason_lines, ["recorded"]);
}

/// Each ending string means one thing, whichever reader interprets it: a round
/// read here and one archived in `issue_history` (`legacy_disposition`).
type EndingCase<'a> = (&'a [(&'a str, &'a str)], &'a str, IssueDisposition);

#[test]
fn every_ending_has_the_disposition_its_text_has_elsewhere() {
    let unreadable = [("verdict.md", "not frontmatter"), ("applied.marker", "")];
    let closed_after_apply = [("applied.marker", ""), (CLOSED_MARKER, "")];
    let closed_with_verdict = [("verdict.md", ACCEPT), (CLOSED_MARKER, "")];
    let closed = [(CLOSED_MARKER, "")];
    let accepted = [("verdict.md", ACCEPT), ("applied.marker", "")];
    let cases: [EndingCase; 5] = [
        (
            &unreadable,
            "ended without a readable verdict",
            IssueDisposition::Closed,
        ),
        (
            &closed_after_apply,
            "closed without a verdict",
            IssueDisposition::Closed,
        ),
        (
            &closed_with_verdict,
            "closed before its verdict was applied",
            IssueDisposition::Closed,
        ),
        (
            &closed,
            "closed without a verdict",
            IssueDisposition::Closed,
        ),
        (
            &accepted,
            "adjudicator verdict: accept",
            IssueDisposition::Accepted,
        ),
    ];
    for (records, ended_by, disposition) in cases {
        let (_temp, work) = work_dir();
        dispute(&work, 1, "x", records);
        let rounds = adjudication_rounds(&stage(), &work);
        assert_eq!(rounds.len(), 1, "{ended_by}");
        assert_eq!(rounds[0].issue.ended_by, ended_by);
        assert_eq!(rounds[0].issue.disposition, Some(disposition), "{ended_by}");
    }
}
