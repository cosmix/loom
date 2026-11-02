//! The rendered lines that report what the estimate could not place.

use super::super::Attribution;
use super::lines;

fn attribution(total_s2: f64, over_attributed_s2: f64, startup_excluded: u64) -> Attribution {
    Attribution {
        total_s2,
        startup_s2: 0.0,
        output_s2: 0.0,
        assistant_replay_s2: 0.0,
        residual_s2: 0.0,
        over_attributed_s2,
        startup_excluded,
        rows: Vec::new(),
        startup_by_agent: Vec::new(),
    }
}

#[test]
fn nothing_to_attribute_renders_only_the_no_data_line() {
    assert_eq!(lines(&attribution(0.0, 0.0, 0)), ["  attribution: no data"]);
}

#[test]
fn a_transcript_with_no_provable_start_is_enough_to_render_the_section() {
    let rendered = lines(&attribution(0.0, 0.0, 2));
    assert!(
        rendered
            .iter()
            .any(|line| line == "  transcripts without a provable start: 2"),
        "{rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|line| line == "  attributed classes: no data"),
        "{rendered:?}"
    );
}

#[test]
fn over_attribution_renders_with_its_amount() {
    let rendered = lines(&attribution(11.0, 1250.0, 0));
    assert!(
        rendered
            .iter()
            .any(|line| line == "  over-attributed S2: 1,250.0"),
        "{rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|line| line == "  transcripts without a provable start: 0"),
        "{rendered:?}"
    );
}
