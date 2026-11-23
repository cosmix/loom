use super::*;

const PLAN: &str =
    "# Plan\n<!-- loom METADATA -->\n```yaml\nx: old\n```\n<!-- END loom METADATA -->\n";

#[test]
fn splice_widens_the_fence_when_the_body_holds_a_closing_line() {
    let extracted = extract_yaml_metadata_with_ranges(PLAN).unwrap();
    let new_yaml_body = "x: 'first\n```\nlast'\n";
    let spliced = splice_metadata_yaml(PLAN, &extracted, new_yaml_body);
    assert!(spliced.contains("\n````yaml\n"));
    assert!(spliced.contains("\n````\n"));
    assert_eq!(
        extract_yaml_metadata_with_ranges(&spliced).unwrap().yaml,
        new_yaml_body.trim()
    );
}

#[test]
fn splice_keeps_the_fence_when_nothing_closes_it() {
    let extracted = extract_yaml_metadata_with_ranges(PLAN).unwrap();
    let new_yaml_body = "x: 'inline ``` backticks'\n";
    let spliced = splice_metadata_yaml(PLAN, &extracted, new_yaml_body);
    assert_eq!(spliced, PLAN.replace("x: old\n", new_yaml_body));
}

#[test]
fn splice_preserves_closing_indentation_and_any_longer_fence() {
    for (closing, expected) in [("```", "````"), ("`````", "`````")] {
        let plan = PLAN.replace("\n```\n", &format!("\n   {closing} \t\r\n"));
        let extracted = extract_yaml_metadata_with_ranges(&plan).unwrap();
        let spliced = splice_metadata_yaml(&plan, &extracted, "x: 'first\n```\nlast'\n");
        assert!(spliced.contains("\n````yaml\n"));
        assert!(spliced.contains(&format!("\n   {expected} \t\r\n")));
    }
}
