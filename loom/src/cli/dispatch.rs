use anyhow::Result;
use loom::commands::{
    clean, diagnose, graph, handoff, init, knowledge, map, memory, plan, pressure, repair, resume,
    review, run, self_update, sessions, skill_index, stage, status, stop, verify, worktree_cmd,
};
use loom::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use std::path::PathBuf;
use std::str::FromStr;

use super::types::{
    Commands, KnowledgeCommands, MemoryCommands, OutputCommands, PlanCommands, SessionsCommands,
    StageCommands, WorktreeCommands,
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
            WorktreeCommands::Remove { stage_id } => worktree_cmd::remove(stage_id),
        },
        Commands::Graph => graph::show(),
        Commands::Handoff {
            stage,
            session,
            trigger,
            message,
        } => handoff::create::execute(stage, session, trigger, message),
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
            StageCommands::Retry {
                stage_id,
                force,
                context,
            } => stage::retry(stage_id, force, context),
            StageCommands::Merge { stage_id, resolved } => stage::merge(stage_id, resolved),
            StageCommands::Verify {
                stage_id,
                no_reload,
                dry_run,
            } => {
                if dry_run {
                    stage::check_acceptance(stage_id)
                } else {
                    stage::verify(stage_id, no_reload)
                }
            }
            StageCommands::HumanReview {
                stage_id,
                approve,
                force_complete,
                reject,
            } => stage::human_review(stage_id, approve, force_complete, reject),
            StageCommands::DisputeCriteria {
                stage_id,
                criterion_index,
                reason,
                evidence_commit,
                failure_output,
            } => stage::dispute_criteria(
                stage_id,
                criterion_index,
                reason,
                evidence_commit,
                failure_output,
            ),
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
            KnowledgeCommands::Audit {
                max_file_lines,
                max_total_lines,
                quiet,
            } => knowledge::audit::audit(max_file_lines, max_total_lines, quiet),
            KnowledgeCommands::Gc {
                model,
                dry_run,
                quick,
            } => knowledge::gc::gc(model, dry_run, quick),
            KnowledgeCommands::Bootstrap {
                model,
                skip_map,
                quick,
            } => knowledge::bootstrap::execute(model, skip_map, quick),
            KnowledgeCommands::ReplaceSection {
                file,
                heading,
                content,
            } => knowledge::replace_section(file, heading, content),
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Note { text, stage } => memory::note(text, stage),
            MemoryCommands::Decision {
                text,
                context,
                stage,
            } => memory::decision(text, context, stage),
            MemoryCommands::Change { text, stage } => memory::change(text, stage),
            MemoryCommands::Question { text, stage } => memory::question(text, stage),
            MemoryCommands::Query { search, stage } => memory::query(search, stage),
            MemoryCommands::List { stage, entry_type } => memory::list(stage, entry_type),
            MemoryCommands::Show { stage, all } => memory::show(stage, all),
        },
        Commands::Review { ai_summary } => review::execute(ai_summary),
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
        Commands::Pressure {
            plan,
            rounds,
            dry_run,
        } => pressure::execute(plan, rounds, dry_run),
        Commands::Stop => stop::execute(),
        Commands::Diagnose { stage_id } => diagnose::execute(&stage_id),
        Commands::Plan { command } => match command {
            PlanCommands::Verify {
                path,
                strict,
                json,
                no_color,
            } => plan::verify::execute(&path, strict, json, no_color),
        },
        Commands::Check { stage_id, suggest } => verify::execute(&stage_id, suggest),
        Commands::SkillIndex => skill_index::execute(),
        Commands::Completions {
            shell,
            install,
            migrate,
        } => {
            if migrate {
                return loom::completions::install::check_migration();
            }

            if install {
                let shell = match shell {
                    Some(s) => Shell::from_str(&s)?,
                    None => loom::completions::install::detect_shell()?,
                };
                return loom::completions::install::install(shell);
            }

            let shell = shell.ok_or_else(|| {
                anyhow::anyhow!("Shell argument required. Usage: loom completions <bash|zsh|fish>")
            })?;
            let shell = Shell::from_str(&shell)?;
            generate_completions(shell);
            Ok(())
        }
        Commands::Complete { shell, args } => {
            let ctx = CompletionContext::from_args(&shell, &args);
            complete_dynamic(&ctx)
        }
    }
}
