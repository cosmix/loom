use crate::commands::{
    attach, clean, context, diagnose, graph, handoff, hook, init, knowledge, map, memory, plan,
    pressure, repair, resume, review, run, self_update, sessions, skill_index, stage, status, stop,
    verify, worktree_cmd,
};
use crate::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use anyhow::Result;
use std::path::PathBuf;
use std::str::FromStr;

use super::types::{
    Commands, ContextCommands, HookCommands, KnowledgeCommands, MemoryCommands, OutputCommands,
    PlanCommands, SessionsCommands, StageCommands, WorktreeCommands,
};

/// The admin proof a `loom stage complete` invocation needs, if any.
///
/// An unprivileged completion needs none. A privileged one authorizes itself
/// through `admin_proof::authorize`, which uses a broker's `LOOM_ADMIN_PROOF`
/// when one is present and otherwise mints from the daemon token the operator
/// can already read. No flag, and nothing for a human to carry between
/// commands.
fn resolve_completion_proof(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> anyhow::Result<Option<String>> {
    if !(no_verify || force_unsafe || assume_merged) {
        return Ok(None);
    }
    stage::admin_proof::authorize(
        std::path::Path::new(".work"),
        stage::admin_proof::AdminProofRequest::completion(
            stage_id,
            no_verify,
            force_unsafe,
            assume_merged,
        ),
    )
}

/// `loom stage admin-proof` — mint one capability and print it, nothing else.
///
/// The secret arrives in `LOOM_ADMIN_TOKEN` and is never read from disk here,
/// so a caller that can invoke loom but cannot read `.work/admin.token` gains
/// nothing: a wrong secret simply mints a proof that verification rejects.
/// That is what separates this command from `admin_proof::authorize`, which
/// reads the token and therefore relies on the sandbox to keep an agent out.
fn print_minted_proof(
    stage_id: Option<String>,
    daemon_stop: bool,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> Result<()> {
    if daemon_stop {
        println!("{}", stage::admin_proof::mint_daemon_stop_proof_from_env()?);
        return Ok(());
    }
    if !no_verify && !force_unsafe && !assume_merged {
        anyhow::bail!("admin-proof requires at least one privileged completion flag");
    }
    let stage_id = stage_id.expect("clap requires stage_id without --daemon-stop");
    println!(
        "{}",
        stage::complete::mint_completion_proof_from_env(
            &stage_id,
            no_verify,
            force_unsafe,
            assume_merged,
        )?
    );
    Ok(())
}

/// `loom knowledge <subcommand>` dispatch.
///
/// Broken out of `dispatch` because this stage's new context/sync
/// subcommands pushed the parent match past the line limit.
fn dispatch_knowledge(command: KnowledgeCommands) -> Result<()> {
    match command {
        KnowledgeCommands::Update { file, content } => knowledge::update(file, content),
        KnowledgeCommands::ReplaceSection {
            file,
            heading,
            content,
        } => knowledge::replace_section(file, heading, content),
        // `budget` is bound short so the seven-argument call stays one line.
        KnowledgeCommands::Context {
            stage,
            query,
            budget_tokens: budget,
            scope,
            require_id,
            explain,
            json,
        } => knowledge::context::context(stage, query, budget, scope, require_id, explain, json),
        KnowledgeCommands::Eval {
            cases,
            budget_tokens,
            json,
        } => knowledge::eval::eval(cases, budget_tokens, json),
        KnowledgeCommands::Sync {
            structural_only,
            json,
        } => knowledge::sync::sync(structural_only, json),
    }
}

/// `loom plan <subcommand>` dispatch.
///
/// Broken out for the same reason as `dispatch_knowledge`: the top-level match
/// sits at its line ceiling, so every new top-level arm has to buy its line back
/// from an existing one.
fn dispatch_plan(command: PlanCommands) -> Result<()> {
    match command {
        PlanCommands::Verify {
            path,
            strict,
            json,
            no_color,
        } => plan::verify::execute(&path, strict, json, no_color),
    }
}

/// `loom context <subcommand>` dispatch.
///
/// These are hook-facing entry points: they run on every agent edit, so they
/// stay quiet on the happy path and degrade instead of failing the tool call
/// that invoked them.
fn dispatch_context(command: ContextCommands) -> Result<()> {
    match command {
        ContextCommands::RecordEdit { stage, paths } => {
            context::record_edit::record_edit(&stage, &paths)
        }
    }
}

/// `loom hook <subcommand>` dispatch.
///
/// The deterministic side of loom's shell hooks: pure filesystem and string
/// work, never a model call and never a network call.
fn dispatch_hook(command: HookCommands) -> Result<()> {
    match command {
        HookCommands::UserPrompt => hook::user_prompt::user_prompt(),
    }
}

/// `loom stage <subcommand>` dispatch.
///
/// Extracted for the same reason as `dispatch_knowledge`: the stage group is
/// the largest arm in the top-level match, which sits at its line ceiling.
fn dispatch_stage(command: StageCommands) -> Result<()> {
    match command {
        StageCommands::Complete {
            stage_id,
            session,
            no_verify,
            force_unsafe,
            assume_merged,
        } => {
            let admin_proof =
                resolve_completion_proof(&stage_id, no_verify, force_unsafe, assume_merged)?;
            stage::complete(
                stage_id,
                session,
                no_verify,
                force_unsafe,
                assume_merged,
                admin_proof,
            )
        }
        StageCommands::AdminProof {
            stage_id,
            daemon_stop,
            no_verify,
            force_unsafe,
            assume_merged,
        } => print_minted_proof(
            stage_id,
            daemon_stop,
            no_verify,
            force_unsafe,
            assume_merged,
        ),
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
        StageCommands::Amend {
            stage_id,
            field,
            op,
            index,
            value,
            reason,
        } => stage::amend(
            stage_id,
            field.to_field(),
            op.to_patch(index, value)?,
            reason,
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
    }
}

pub fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Init {
            plan_path,
            clean,
            backend,
            allow_unsafe_plan,
        } => init::execute(
            Some(PathBuf::from(plan_path)),
            clean,
            backend,
            allow_unsafe_plan,
        ),
        Commands::Run {
            manual,
            max_parallel,
            foreground,
            watch,
            no_merge,
            backend,
        } => {
            let auto_merge = !no_merge;
            if foreground {
                run::execute(manual, max_parallel, watch, auto_merge, backend)
            } else {
                run::execute_background(manual, max_parallel, auto_merge, backend)
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
        Commands::Attach { stage_id } => attach::execute(stage_id),
        Commands::Worktree { command } => match command {
            WorktreeCommands::List => worktree_cmd::list(),
            WorktreeCommands::Remove {
                stage_id,
                force,
                confirmation,
            } => worktree_cmd::remove(stage_id, force, confirmation),
        },
        Commands::Graph => graph::show(),
        Commands::Handoff {
            stage,
            session,
            trigger,
            message,
        } => handoff::create::execute(stage, session, trigger, message),
        Commands::Stage { command } => dispatch_stage(command),
        Commands::Knowledge { command } => dispatch_knowledge(command),
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
        Commands::Map { args } => map::execute(args),
        Commands::Pressure {
            plan,
            rounds,
            dry_run,
        } => pressure::execute(plan, rounds, dry_run),
        Commands::Stop => stop::execute(),
        Commands::Diagnose { stage_id } => diagnose::execute(&stage_id),
        Commands::Plan { command } => dispatch_plan(command),
        Commands::Check { stage_id, suggest } => verify::execute(&stage_id, suggest),
        Commands::SkillIndex => skill_index::execute(),
        Commands::Completions {
            shell,
            install,
            migrate,
        } => {
            if migrate {
                return crate::completions::install::check_migration();
            }

            if install {
                let shell = match shell {
                    Some(s) => Shell::from_str(&s)?,
                    None => crate::completions::install::detect_shell()?,
                };
                return crate::completions::install::install(shell);
            }

            let shell = shell.ok_or_else(|| {
                anyhow::anyhow!("Shell argument required. Usage: loom completions <bash|zsh|fish>")
            })?;
            let shell = Shell::from_str(&shell)?;
            generate_completions(shell);
            Ok(())
        }
        Commands::Context { command } => dispatch_context(command),
        Commands::Hook { command } => dispatch_hook(command),
        Commands::Complete { shell, args } => {
            let ctx = CompletionContext::from_args(&shell, &args);
            complete_dynamic(&ctx)
        }
    }
}
