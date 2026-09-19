//! Skill recommendation section of a stage signal: per-skill invocations plus one combined loader call.

use crate::skills::{is_core_skill, skill_invocation, SkillMatch};

/// Format task progression information for inclusion in signals
pub fn format_skill_recommendations(skills: &[SkillMatch]) -> String {
    let mut content = String::new();

    content.push_str("## Recommended Skills\n\n");

    // Partition skills into three classes with different framing:
    // - `declared`: skills the plan's `skills:` field names for this stage.
    //   These are a DIRECTIVE, and lead the section — the plan author's
    //   explicit requirement outranks anything inferred.
    // - `detected`: language skills inferred from the files this stage edits.
    //   Also a DIRECTIVE — load them before writing code.
    // - `advisory`: skills matched from the task description. Invoke if relevant.
    let (declared, rest): (Vec<&SkillMatch>, Vec<&SkillMatch>) = skills
        .iter()
        .partition(|s| s.matched_triggers.iter().any(|t| t == "declared-for-stage"));
    let (detected, advisory): (Vec<&SkillMatch>, Vec<&SkillMatch>) =
        rest.into_iter().partition(|s| {
            s.matched_triggers
                .iter()
                .any(|t| t == "project-language" || t.starts_with("project-type:"))
        });

    if !declared.is_empty() {
        content.push_str(&format_declared_skills(&declared));
    }

    if !detected.is_empty() {
        content.push_str(&format_detected_skills(&detected));
    }

    if !advisory.is_empty() {
        content.push_str(&format_advisory_skills(&advisory));
    }

    // Advisory matches remain optional, so only the plan's declared skills and
    // direct project detections may be grouped into an all-at-once loader
    // directive.
    let must_load: Vec<&SkillMatch> = declared.iter().chain(detected.iter()).copied().collect();
    if let Some(line) = combined_loader_line(&must_load) {
        content.push_str(&line);
    }

    content
}

/// Render the "load now" directive block for skills the plan declared as
/// required for this stage.
fn format_declared_skills(declared: &[&SkillMatch]) -> String {
    let mut content = String::new();
    content.push_str(
        "**Required for this stage — load these now.** The plan declares these skills for \
         your work here; invoke the Skill tool for each:\n\n",
    );
    for skill in declared {
        content.push_str(&format!("- `{}`\n", skill_invocation(&skill.name)));
    }
    content.push('\n');
    content
}

/// Render the "load now" directive block for skills inferred from the file
/// types this stage edits.
fn format_detected_skills(detected: &[&SkillMatch]) -> String {
    let mut content = String::new();
    content.push_str(
        "**Load these now — before editing any files.** Based on the file types this \
         stage will edit, invoke the Skill tool for each so your code follows the \
         project's language and framework conventions:\n\n",
    );
    // Claude Code indexes only the core skills, so a catalogued one has no
    // `Skill(skill="loom-rust")` of its own. `skill_invocation` renders the
    // loom-skills loader call for those, and the plain call for the rest.
    for skill in detected {
        content.push_str(&format!("- `{}`\n", skill_invocation(&skill.name)));
    }
    content.push('\n');
    content
}

/// Render the advisory table + matched-triggers block for skills matched
/// from the task description.
fn format_advisory_skills(advisory: &[&SkillMatch]) -> String {
    let mut content = String::new();
    content.push_str("These skills may also help with your task — invoke any that apply:\n\n");
    content.push_str("| Skill | Description | Invoke |\n");
    content.push_str("|-------|-------------|--------|\n");

    for skill in advisory {
        // Truncate description if too long for table (UTF-8 safe)
        let desc = if skill.description.chars().count() > 60 {
            format!(
                "{}...",
                skill.description.chars().take(57).collect::<String>()
            )
        } else {
            skill.description.clone()
        };
        // Escape pipe characters in description and name
        let desc = desc.replace('|', "\\|");
        let invoke = skill_invocation(&skill.name);
        let name = skill.name.replace('|', "\\|");
        content.push_str(&format!("| {} | {} | `{}` |\n", name, desc, invoke));
    }
    content.push('\n');

    // Show which triggers matched for transparency
    content.push_str("**Matched triggers:**\n");
    for skill in advisory {
        if !skill.matched_triggers.is_empty() {
            let triggers = skill.matched_triggers.join(", ");
            content.push_str(&format!("- `{}`: {}\n", skill.name, triggers));
        }
    }
    content.push('\n');

    content
}

/// One combined `loom-skills` loader call naming every catalogued (non-core)
/// skill in `skills`, in the given order. `None` when fewer than two qualify
/// — a single catalogued skill already has its own invocation line above,
/// and a core skill has no catalog entry for the loader to read.
fn combined_loader_line(skills: &[&SkillMatch]) -> Option<String> {
    let names: Vec<&str> = skills
        .iter()
        .map(|s| s.name.as_str())
        .filter(|name| !is_core_skill(name))
        .collect();

    if names.len() < 2 {
        return None;
    }

    Some(format!(
        "**Load all catalogued ones at once:** `Skill(skill=\"loom-skills\", args=\"{}\")`\n\n",
        names.join(" ")
    ))
}

#[cfg(test)]
mod skill_recommendation_tests {
    use super::format_skill_recommendations;
    use crate::skills::SkillMatch;

    fn detected(name: &str) -> SkillMatch {
        SkillMatch::new(
            name.to_string(),
            "Language expertise".to_string(),
            10.0,
            vec!["project-language".to_string()],
        )
    }

    fn advisory(name: &str, trigger: &str) -> SkillMatch {
        SkillMatch::new(
            name.to_string(),
            "Some advisory skill".to_string(),
            2.0,
            vec![trigger.to_string()],
        )
    }

    fn declared(name: &str) -> SkillMatch {
        SkillMatch::new(
            name.to_string(),
            "Declared for this stage".to_string(),
            10.0,
            vec!["declared-for-stage".to_string()],
        )
    }

    #[test]
    fn declared_skills_render_before_detected_with_required_wording() {
        let out = format_skill_recommendations(&[detected("loom-rust"), declared("loom-auth")]);
        assert!(
            out.contains("Required for this stage"),
            "missing required framing: {out}"
        );
        let declared_pos = out
            .find("Required for this stage")
            .expect("declared present");
        let detected_pos = out.find("Load these now").expect("detected present");
        assert!(
            declared_pos < detected_pos,
            "declared skills should precede detected: {out}"
        );
        assert!(
            out.contains("Skill(skill=\"loom-skills\", args=\"loom-auth\")"),
            "missing declared skill invocation: {out}"
        );
    }

    #[test]
    fn declared_and_detected_share_the_combined_loader_line() {
        let out = format_skill_recommendations(&[declared("loom-auth"), detected("loom-rust")]);
        assert!(
            out.contains(
                "**Load all catalogued ones at once:** \
                 `Skill(skill=\"loom-skills\", args=\"loom-auth loom-rust\")`"
            ),
            "declared and detected skills should share one combined loader line: {out}"
        );
    }

    #[test]
    fn detected_skills_render_as_skill_tool_directive() {
        let out = format_skill_recommendations(&[detected("loom-rust")]);
        // Directive framing + an explicit Skill tool invocation the agent can run.
        assert!(out.contains("Load these now"), "missing directive: {out}");
        assert!(
            out.contains("Skill(skill=\"loom-skills\", args=\"loom-rust\")"),
            "missing Skill tool call: {out}"
        );
    }

    #[test]
    fn advisory_skills_render_as_table_not_directive() {
        let out = format_skill_recommendations(&[
            advisory("loom-auth", "jwt"),
            advisory("loom-search", "search"),
        ]);
        assert!(
            !out.contains("Load these now"),
            "should not be directive: {out}"
        );
        assert!(
            out.contains("may also help"),
            "missing advisory framing: {out}"
        );
        assert!(
            out.contains("Skill(skill=\"loom-skills\", args=\"loom-auth\")"),
            "missing invoke column: {out}"
        );
        assert!(out.contains("jwt"), "missing matched trigger: {out}");
        assert!(
            !out.contains("Load all catalogued ones at once"),
            "advisory-only matches must not get a combined directive: {out}"
        );
    }

    #[test]
    fn detected_and_advisory_are_partitioned() {
        let out =
            format_skill_recommendations(&[detected("loom-rust"), advisory("loom-auth", "jwt")]);
        // Detected directive comes before the advisory table.
        let load_pos = out.find("Load these now").expect("directive present");
        let advisory_pos = out.find("may also help").expect("advisory present");
        assert!(
            load_pos < advisory_pos,
            "directive should precede advisory: {out}"
        );
        assert!(
            !out.contains("Load all catalogued ones at once"),
            "advisory skills must not become part of a combined directive: {out}"
        );
    }

    #[test]
    fn two_catalogued_skills_get_a_combined_loader_line() {
        let out = format_skill_recommendations(&[detected("loom-rust"), detected("loom-auth")]);
        assert!(
            out.contains(
                "**Load all catalogued ones at once:** \
                 `Skill(skill=\"loom-skills\", args=\"loom-rust loom-auth\")`"
            ),
            "missing combined loader line for detected skills: {out}"
        );
    }

    #[test]
    fn combined_loader_excludes_catalogued_advisory_skills() {
        let out = format_skill_recommendations(&[
            detected("loom-rust"),
            detected("loom-auth"),
            advisory("loom-search", "search"),
        ]);
        let combined = out
            .lines()
            .find(|line| line.contains("Load all catalogued ones at once"))
            .expect("combined loader line");
        assert!(
            combined.contains("args=\"loom-rust loom-auth\""),
            "detected skills missing: {combined}"
        );
        assert!(
            !combined.contains("loom-search"),
            "advisory skill leaked into combined loader: {combined}"
        );
    }

    #[test]
    fn single_catalogued_skill_has_no_combined_loader_line() {
        let out = format_skill_recommendations(&[detected("loom-rust")]);
        assert!(
            !out.contains("Load all catalogued ones at once"),
            "combined line should not appear for a single skill: {out}"
        );
    }

    #[test]
    fn core_skill_is_excluded_from_combined_loader_line() {
        // loom-plan-writer is a core skill (skills/core-skills.txt) — it has
        // no catalog entry and should never appear in the combined args.
        let out = format_skill_recommendations(&[
            detected("loom-rust"),
            advisory("loom-plan-writer", "plan"),
        ]);
        assert!(
            !out.contains("Load all catalogued ones at once"),
            "combined line should not appear with only one catalogued skill: {out}"
        );
        let out2 = format_skill_recommendations(&[
            detected("loom-rust"),
            detected("loom-auth"),
            advisory("loom-plan-writer", "plan"),
        ]);
        assert!(
            out2.contains("args=\"loom-rust loom-auth\""),
            "combined args should hold only catalogued skills: {out2}"
        );
    }
}
