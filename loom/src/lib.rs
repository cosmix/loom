pub mod checkpoints;
pub mod commands;
pub mod completions;
pub mod daemon;
pub mod diagnosis;
pub mod fs;
pub mod git;
pub mod handoff;
pub mod map;
pub mod models;
pub mod orchestrator;
pub mod parser;
pub mod plan;
pub mod process;
pub mod sandbox;
pub mod skills;
pub mod utils;
pub mod validation;
pub mod verify;

/// ASCII art logo for loom CLI
pub const LOGO: &str = "\
   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴";
