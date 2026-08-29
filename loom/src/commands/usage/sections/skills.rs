//! Counts explicit skill invocations, helping maintainers decide which shared
//! workflows earn continued upkeep and which only add prompt overhead.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::Transcript;
use crate::context::untrusted::inline_safe;

use super::fmt::{heading, no_data};

#[derive(Debug, serde::Serialize)]
pub struct Skills {
    pub invocations: Vec<SkillRow>,
}

#[derive(Debug, serde::Serialize)]
pub struct SkillRow {
    pub skill: String,
    pub count: usize,
}

pub fn build(transcripts: &[Transcript]) -> Skills {
    let mut counts = BTreeMap::<String, usize>::new();
    for request in transcripts.iter().flat_map(Transcript::requests) {
        for tool in &request.tool_uses {
            if tool.name == "Skill" {
                let skill = tool
                    .input
                    .get("skill")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("(unknown)");
                *counts.entry(skill.to_owned()).or_default() += 1;
            }
        }
    }
    let mut invocations = counts
        .into_iter()
        .map(|(skill, count)| SkillRow { skill, count })
        .collect::<Vec<_>>();
    invocations.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.skill.cmp(&right.skill))
    });
    Skills { invocations }
}

pub fn render(skills: &Skills) {
    heading("Skills");
    if skills.invocations.is_empty() {
        no_data("skill invocations");
        return;
    }
    for skill in &skills.invocations {
        println!("  {}: {}", inline_safe(&skill.skill), skill.count);
    }
}
