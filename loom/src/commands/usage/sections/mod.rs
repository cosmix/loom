pub mod agents;
pub mod by_model;
pub mod edits;
pub mod fmt;
pub mod lengths;
pub mod lifecycle;
pub mod polling;
pub mod reads;
pub mod rewrites;
pub mod skills;
pub mod tools;
pub mod totals;
pub mod windows;

use crate::commands::usage::accounting::Windowing;
use crate::commands::usage::transcript::Transcript;

#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub totals: totals::Totals,
    pub windows: windows::WindowReport,
    pub by_model: by_model::ByModel,
    pub lengths: lengths::SessionLengths,
    pub tools: tools::ToolShape,
    pub reads: reads::Reads,
    pub agents: agents::AgentReport,
    pub skills: skills::Skills,
    pub polling: polling::Polling,
    pub rewrites: rewrites::CacheRewrites,
    pub lifecycle: lifecycle::Lifecycle,
    pub edits: edits::EditRequests,
}

pub fn build(transcripts: &[Transcript], windowing: Windowing) -> Report {
    Report {
        totals: totals::build(transcripts),
        windows: windows::build(transcripts, windowing),
        by_model: by_model::build(transcripts),
        lengths: lengths::build(transcripts),
        tools: tools::build(transcripts),
        reads: reads::build(transcripts),
        agents: agents::build(transcripts),
        skills: skills::build(transcripts),
        polling: polling::build(transcripts),
        rewrites: rewrites::build(transcripts),
        lifecycle: lifecycle::build(transcripts),
        edits: edits::build(transcripts),
    }
}

pub fn render(report: &Report) {
    totals::render(&report.totals);
    windows::render(&report.windows);
    by_model::render(&report.by_model);
    lengths::render(&report.lengths);
    tools::render(&report.tools);
    reads::render(&report.reads);
    agents::render(&report.agents);
    skills::render(&report.skills);
    polling::render(&report.polling);
    rewrites::render(&report.rewrites);
    lifecycle::render(&report.lifecycle);
    edits::render(&report.edits);
}
