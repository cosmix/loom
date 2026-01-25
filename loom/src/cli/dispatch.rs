use anyhow::Result;
use loom::checkpoints::CheckpointStatus;
use loom::commands::{
    checkpoint, clean, diagnose, graph, hooks, init, knowledge, memory, merge, resume, run,
    self_update, sessions, stage, status, stop, verify, worktree_cmd,
};
use loom::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use std::path::PathBuf;
use std::str::FromStr;

use super::types::{
    CheckpointCommands, Cli, Commands, GraphCommands, HooksCommands, KnowledgeCommands,
    MemoryCommands, OutputCommands, SessionsCommands, StageCommands, WorktreeCommands,
};

pub fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Init { plan_path, clean } => init::execute(Some(PathBuf::from(plan_path)), clean),
        Commands::Run {
            manual,
            max_parallel,
            foreground,
            watch,
            no_merge,
        } => {
            let auto_merge = !no_merge;
            if foreground {
                run::execute(manual, max_parallel, watch, auto_merge)
            } else {
                run::execute_background(manual, max_parallel, watch, auto_merge)
            }
        }
        Commands::Status {
            live,
            compact,
            verbose,
        } => status::execute(live, compact, verbose),
        Commands::Resume { stage_id } => resume::execute(stage_id),
        Commands::Merge { stage_id, force } => merge::execute(stage_id, force),
        Commands::Sessions { command } => match command {
            SessionsCommands::List => sessions::list(),
            SessionsCommands::Kill { session_ids, stage } => sessions::kill(session_ids, stage),
        },
        Commands::Worktree { command } => match command {
            WorktreeCommands::List => worktree_cmd::list(),
            WorktreeCommands::Clean => worktree_cmd::clean(),
            WorktreeCommands::Remove { stage_id } => worktree_cmd::remove(stage_id),
        },
        Commands::Graph { command } => match command {
            GraphCommands::Show => graph::show(),
            GraphCommands::Edit => graph::edit(),
        },
        Commands::Hooks { command } => match command {
            HooksCommands::Install => hooks::install(),
            HooksCommands::List => hooks::list(),
        },
        Commands::Stage { command } => match command {
            StageCommands::Complete {
                stage_id,
                session,
                no_verify,
                force_unsafe,
                assume_merged,
            } => stage::complete(stage_id, session, no_verify, force_unsafe, assume_merged),
            StageCommands::Block { stage_id, reason } => stage::block(stage_id, reason),
            StageCommands::Reset {
                stage_id,
                hard,
                kill_session,
            } => stage::reset(stage_id, hard, kill_session),
            StageCommands::Waiting { stage_id } => stage::waiting(stage_id),
            StageCommands::Resume { stage_id } => stage::resume_from_waiting(stage_id),
            StageCommands::Hold { stage_id } => stage::hold(stage_id),
            StageCommands::Release { stage_id } => stage::release(stage_id),
            StageCommands::Skip { stage_id, reason } => stage::skip(stage_id, reason),
            StageCommands::Retry { stage_id, force } => stage::retry(stage_id, force),
            StageCommands::Recover { stage_id, force } => stage::recover(stage_id, force),
            StageCommands::MergeComplete { stage_id } => stage::merge_complete(stage_id),
            StageCommands::Verify {
                stage_id,
                no_reload,
            } => stage::verify(stage_id, no_reload),
            StageCommands::Output { command } => match command {
                OutputCommands::Set {
                    stage_id,
                    key,
                    value,
                    description,
                } => stage::output_set(stage_id, key, value, description),
                OutputCommands::Get { stage_id, key } => stage::output_get(stage_id, key),
                OutputCommands::List { stage_id } => stage::output_list(stage_id),
                OutputCommands::Remove { stage_id, key } => stage::output_remove(stage_id, key),
            },
        },
        Commands::Knowledge { command } => match command {
            KnowledgeCommands::Show { file } => knowledge::show(file),
            KnowledgeCommands::Update { file, content } => knowledge::update(file, content),
            KnowledgeCommands::Init => knowledge::init(),
            KnowledgeCommands::List => knowledge::list(),
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Note { text, session } => memory::note(text, session),
            MemoryCommands::Decision {
                text,
                context,
                session,
            } => memory::decision(text, context, session),
            MemoryCommands::Question { text, session } => memory::question(text, session),
            MemoryCommands::Query { search, session } => memory::query(search, session),
            MemoryCommands::List {
                session,
                entry_type,
            } => memory::list(session, entry_type),
            MemoryCommands::Show { session } => memory::show(session),
            MemoryCommands::Sessions => memory::sessions(),
            MemoryCommands::Promote {
                entry_type,
                target,
                session,
            } => memory::promote(entry_type, target, session),
        },
        Commands::SelfUpdate => self_update::execute(),
        Commands::Clean {
            all,
            worktrees,
            sessions,
            state,
        } => clean::execute(all, worktrees, sessions, state),
        Commands::Stop => stop::execute(),
        Commands::Checkpoint { command } => match command {
            CheckpointCommands::Create {
                task_id,
                status,
                force,
                outputs,
                notes,
            } => {
                let status = status.parse::<CheckpointStatus>()?;
                checkpoint::execute(task_id, status, force, outputs, notes)
            }
            CheckpointCommands::List { session } => checkpoint::list(session),
        },
        Commands::Diagnose { stage_id } => diagnose::execute(&stage_id),
        Commands::Verify { stage_id, suggest } => verify::execute(&stage_id, suggest),
        Commands::Completions { shell } => {
            let shell = Shell::from_str(&shell)?;
            let mut cmd = <Cli as clap::CommandFactory>::command();
            generate_completions(&mut cmd, shell);
            Ok(())
        }
        Commands::Complete { shell, args } => {
            let ctx = CompletionContext::from_args(&shell, &args);
            complete_dynamic(&ctx)
        }
    }
}
