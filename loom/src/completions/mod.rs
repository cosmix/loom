pub mod dynamic;
pub mod generator;
pub mod install;
pub mod scripts;

pub use dynamic::{
    complete_commands, complete_dynamic, complete_flags, complete_knowledge_files,
    complete_model_names, complete_plan_files, complete_session_ids, complete_shell_types,
    complete_stage_ids, complete_stage_ids_filtered, complete_stage_or_session_ids,
    complete_subcommands, complete_trigger_types, CompletionContext,
};
pub use generator::{generate_completions, Shell};
