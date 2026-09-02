#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod claude;
pub mod cli;
pub mod codex;
pub mod commands;
pub mod completions;
pub mod context;
pub mod daemon;
pub mod diagnosis;
pub mod fs;
pub mod git;
pub mod handoff;
pub mod hooks;
pub mod language;
pub mod map;
pub mod models;
pub mod orchestrator;
pub mod parser;
pub mod plan;
pub mod process;
pub mod remote_control;
pub mod sandbox;
pub mod skills;
pub mod telemetry;
pub mod user_config;
pub mod utils;
pub mod validation;
pub mod verify;
pub mod version;

/// ASCII art logo for loom CLI
pub const LOGO: &str = "   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴";
