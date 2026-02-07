use anyhow::Result;
use loom::commands::{
    clean, diagnose, graph, handoff, hooks, init, knowledge, map, memory, repair, resume, run,
    sandbox, self_update, sessions, stage, status, stop, verify, worktree_cmd,
};
use loom::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use std::path::PathBuf;
use std::str::FromStr;

use super::types::{
    Cli, Commands, GraphCommands, HandoffCommands, HooksCommands, KnowledgeCommands,
    MemoryCommands, OutputCommands, SandboxCommands, SessionsCommands, StageCommands,
    WorktreeCommands,
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
        Commands::Handoff { command } => match command {
            HandoffCommands::Create {
                stage,
                session,
                trigger,
                message,
            } => handoff::create::execute(stage, session, trigger, message),
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
            StageCommands::RetryMerge { stage_id } => stage::retry_merge(stage_id),
            StageCommands::Verify {
                stage_id,
                no_reload,
            } => stage::verify(stage_id, no_reload),
            StageCommands::CheckAcceptance { stage_id } => stage::check_acceptance(stage_id),
            StageCommands::HumanReview {
                stage_id,
                approve,
                force_complete,
                reject,
            } => stage::human_review(stage_id, approve, force_complete, reject),
            StageCommands::DisputeCriteria { stage_id, reason } => {
                stage::dispute_criteria(stage_id, reason)
            }
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
            KnowledgeCommands::Check {
                min_coverage,
                src_path,
                quiet,
            } => knowledge::check::check(min_coverage, src_path, quiet),
            KnowledgeCommands::Gc {
                max_file_lines,
                max_total_lines,
                quiet,
            } => knowledge::gc::gc(max_file_lines, max_total_lines, quiet),
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Note { text, stage } => memory::note(text, stage),
            MemoryCommands::Decision {
                text,
                context,
                stage,
            } => memory::decision(text, context, stage),
            MemoryCommands::Question { text, stage } => memory::question(text, stage),
            MemoryCommands::Query { search, stage } => memory::query(search, stage),
            MemoryCommands::List { stage, entry_type } => memory::list(stage, entry_type),
            MemoryCommands::Show { stage, all } => memory::show(stage, all),
        },
        Commands::Sandbox { command } => match command {
            SandboxCommands::Suggest => sandbox::suggest(),
        },
        Commands::SelfUpdate => self_update::execute(),
        Commands::Clean {
            all,
            worktrees,
            sessions,
            state,
        } => clean::execute(all, worktrees, sessions, state),
        Commands::Repair { fix } => repair::execute(fix),
        Commands::Map {
            deep,
            focus,
            overwrite,
        } => map::execute(deep, focus, overwrite),
        Commands::Stop => stop::execute(),
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
