use crate::commands::{
    ask, attach, clean, config, context, graph, handoff, hook, init, install_assets, knowledge,
    map, memory, plan, pressure, project, repair, request, resume, review, run, self_update,
    sessions, skill_index, status, stop, subagents, target, usage, verify, worktree_cmd,
};
use crate::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use anyhow::Result;
use std::path::PathBuf;
use std::str::FromStr;

use super::dispatch_admin;
use super::dispatch_stage;
use super::types::{
    Commands, CompletionsArgs, ContextCommands, HookCommands, KnowledgeCommands, MemoryCommands,
    PlanCommands, PlanServeArgs, ProjectCommands, RequestCommands, RunArgs, SessionsCommands,
    StatusArgs, TargetCommands, WorktreeCommands,
};
use super::types_memory::KnowledgeContextArgs;

// `dispatch_stage` reaches these through `super::dispatch::{..}`; re-exporting
// here keeps that path working now that the implementations live in
// `dispatch_admin`.
pub(super) use dispatch_admin::{print_minted_proof, resolve_completion_proof};

/// `loom knowledge <subcommand>` dispatch.
fn dispatch_knowledge(command: KnowledgeCommands) -> Result<()> {
    match command {
        KnowledgeCommands::RecoverBaseline { stage, commit } => {
            knowledge::recovery::recover_baseline(&stage, &commit)
        }
        KnowledgeCommands::Update { file, content } => knowledge::update(file, content),
        KnowledgeCommands::ReplaceSection {
            file,
            heading,
            content,
        } => knowledge::replace_section(file, heading, content),
        KnowledgeCommands::DeleteSection { file, heading } => {
            knowledge::delete_section(file, heading)
        }
        KnowledgeCommands::Annotate(args) => knowledge::annotate::annotate(args),
        KnowledgeCommands::Context(args) => dispatch_knowledge_context(args),
        KnowledgeCommands::Eval {
            cases,
            budget_tokens,
            json,
        } => knowledge::eval::eval(cases, budget_tokens, json),
        KnowledgeCommands::Telemetry { stage, json } => {
            knowledge::telemetry::telemetry(stage, json)
        }
        KnowledgeCommands::Sync {
            structural_only,
            json,
        } => knowledge::sync::sync(structural_only, json),
        KnowledgeCommands::Bootstrap(args) => knowledge::bootstrap::execute(args),
        KnowledgeCommands::Check {
            strict,
            strict_evidence,
            json,
            baseline,
            write_baseline,
        } => knowledge::check::check(knowledge::check::CheckOptions {
            strict,
            strict_evidence,
            json,
            baseline,
            write_baseline,
        }),
    }
}

fn dispatch_knowledge_context(args: KnowledgeContextArgs) -> Result<()> {
    knowledge::context::context(
        args.stage,
        args.query,
        args.budget_tokens,
        args.scope,
        args.require_id,
        args.history,
        args.require_compact,
        args.explain,
        args.json,
    )
}

/// `loom plan [<subcommand>]` dispatch: bare `loom plan` serves the planning app.
fn dispatch_plan(command: Option<PlanCommands>, serve: PlanServeArgs) -> Result<()> {
    match command {
        None => plan::serve::execute(&serve),
        Some(PlanCommands::Verify {
            path,
            strict,
            json,
            no_color,
        }) => plan::verify::execute(&path, strict, json, no_color),
        Some(PlanCommands::Check { json }) => crate::commands::plan::check::execute(json),
        Some(PlanCommands::Hook { event, session_dir }) => {
            crate::commands::plan::hook::execute(&event, &session_dir)
        }
        Some(PlanCommands::Summary { command }) => crate::commands::plan::summary::execute(command),
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
        HookCommands::ReconcileGraph { cancel } => hook::reconcile_graph::reconcile_graph(cancel),
        HookCommands::PreCompact => hook::pre_compact::pre_compact(),
        HookCommands::ContextCeilings => hook::context_ceilings::context_ceilings(),
        HookCommands::ForwardReceipt => hook::forward_receipt::forward_receipt(),
        HookCommands::WorkerBrief => hook::worker_brief::worker_brief(),
        HookCommands::ReviewerModel => hook::reviewer_model::reviewer_model(),
        HookCommands::ProjectTypes => hook::project_types::execute(),
        HookCommands::Relay { allowed_kinds } => hook::relay::relay(&allowed_kinds),
        HookCommands::ReviewHarvest => hook::review_harvest::review_harvest(),
        HookCommands::ReviewStart => hook::review_start::review_start(),
    }
}

/// `loom request <subcommand>` dispatch.
fn dispatch_request(command: RequestCommands) -> Result<()> {
    match command {
        RequestCommands::Status { id, session, wait } => {
            request::status::execute(id, session, wait)
        }
    }
}

/// `loom completions` dispatch.
fn dispatch_completions(args: CompletionsArgs) -> Result<()> {
    if args.migrate {
        return crate::completions::install::check_migration();
    }

    if args.install {
        let shell = match args.shell {
            Some(s) => Shell::from_str(&s)?,
            None => crate::completions::install::detect_shell()?,
        };
        return crate::completions::install::install(shell);
    }

    let shell = args.shell.ok_or_else(|| {
        anyhow::anyhow!("Shell argument required. Usage: loom completions <bash|zsh|fish>")
    })?;
    let shell = Shell::from_str(&shell)?;
    generate_completions(shell);
    Ok(())
}

/// `loom complete` dispatch.
fn dispatch_complete(shell: &str, args: &[String]) -> Result<()> {
    complete_dynamic(&CompletionContext::from_args(shell, args))
}

/// Apply the cache option before running `loom check`.
fn run_check(stage_id: String, suggest: bool, args: verify::CheckArgs) -> Result<()> {
    if args.no_cache {
        // Single-threaded at this point in a one-shot CLI invocation — no
        // concurrent reader of the environment exists yet — so a plain
        // process-wide set is safe. `CriteriaConfig::default()` (built inside
        // `run_acceptance_with_display`) reads this once.
        std::env::set_var("LOOM_ACCEPTANCE_CACHE", "0");
    }
    verify::execute(&stage_id, suggest, &args.criterion)
}

/// `loom sessions <subcommand>` dispatch.
fn dispatch_sessions(command: SessionsCommands) -> Result<()> {
    match command {
        SessionsCommands::List => sessions::list(),
        SessionsCommands::Kill { session_ids, stage } => sessions::kill(session_ids, stage),
    }
}

/// `loom worktree <subcommand>` dispatch.
fn dispatch_worktree(command: WorktreeCommands) -> Result<()> {
    match command {
        WorktreeCommands::List => worktree_cmd::list(),
        WorktreeCommands::Remove {
            stage_id,
            force,
            confirmation,
        } => worktree_cmd::remove(stage_id, force, confirmation),
    }
}

/// `loom memory <subcommand>` dispatch.
fn dispatch_memory(command: MemoryCommands) -> Result<()> {
    match command {
        MemoryCommands::Note {
            text,
            evidence,
            stage,
        } => memory::note(text, evidence, stage),
        MemoryCommands::Decision {
            text,
            context,
            evidence,
            stage,
        } => memory::decision(text, context, evidence, stage),
        MemoryCommands::Change {
            text,
            evidence,
            stage,
        } => memory::change(text, evidence, stage),
        MemoryCommands::Question {
            text,
            evidence,
            stage,
        } => memory::question(text, evidence, stage),
        MemoryCommands::Resolve {
            event_id,
            outcome,
            target,
            reason,
            stage,
        } => memory::resolve(event_id, outcome, target, reason, stage),
        MemoryCommands::Pending {
            stage,
            json,
            strict,
            group,
        } => memory::pending(stage, json, strict, group),
        MemoryCommands::Query { search, stage } => memory::query(search, stage),
        MemoryCommands::List {
            stage,
            entry_type,
            json,
        } => memory::list(stage, entry_type, json),
        MemoryCommands::Show { stage, all, json } => memory::show(stage, all, json),
    }
}

/// `loom target <subcommand>` dispatch.
fn dispatch_target(command: TargetCommands) -> Result<()> {
    match command {
        TargetCommands::Status => target::status::execute(),
        TargetCommands::Accept { to } => target::accept::execute(&to),
    }
}

fn dispatch_run(args: RunArgs) -> Result<()> {
    let auto_merge = !args.no_merge;
    if args.foreground {
        run::execute(
            args.manual,
            args.max_parallel,
            args.watch,
            auto_merge,
            args.backend,
            args.accept_unix_socket_risk,
        )
    } else {
        run::execute_background(
            args.manual,
            args.max_parallel,
            auto_merge,
            args.backend,
            args.accept_unix_socket_risk,
            args.daemon_child,
        )
    }
}

fn dispatch_status(args: StatusArgs) -> Result<()> {
    let web_args = args.web_args;
    match web_args.web {
        Some(port) => status::web::execute(port, web_args.terminals, web_args.resolved_host()),
        None => status::execute(args.live, args.compact, args.verbose),
    }
}

fn dispatch_project(command: ProjectCommands) -> Result<()> {
    let ProjectCommands::Detect { path, json } = command;
    project::execute(path, json)
}

pub fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Init {
            plan_path,
            clean,
            backend,
            no_repair,
        } => init::execute(Some(PathBuf::from(plan_path)), clean, backend, no_repair),
        Commands::Run(args) => dispatch_run(args),
        Commands::Status(args) => dispatch_status(args),
        Commands::Resume { stage_id } => resume::execute(stage_id),
        Commands::Sessions { command } => dispatch_sessions(command),
        Commands::Worktree { command } => dispatch_worktree(command),
        Commands::Attach { stage_id } => attach::execute(stage_id),
        Commands::Target { command } => dispatch_target(command),
        Commands::Graph => graph::show(),
        Commands::Handoff {
            stage,
            session,
            trigger,
            message,
        } => handoff::create::execute(stage, session, trigger, message),
        Commands::Stage { command } => dispatch_stage::dispatch_stage(command),
        Commands::Knowledge { command } => dispatch_knowledge(command),
        Commands::Memory { command } => dispatch_memory(command),
        Commands::Request { command } => dispatch_request(command),
        Commands::Review { ai_summary } => review::execute(ai_summary),
        Commands::Update => self_update::execute(),
        Commands::InstallAssets { skills } => install_assets::execute(skills),
        Commands::Config(args) => config::execute(args),
        Commands::Clean {
            all,
            worktrees,
            sessions,
            state,
        } => clean::execute(all, worktrees, sessions, state),
        Commands::Repair { fix } => repair::execute(fix),
        Commands::Ask { args } => ask::execute(args),
        Commands::Map { args } => map::execute(args),
        Commands::Subagents { args } => subagents::execute(args),
        Commands::Usage { args } => usage::execute(args),
        Commands::Pressure(args) => pressure::execute(args),
        Commands::Stop => stop::execute(),
        Commands::Plan { command, serve } => dispatch_plan(command, serve),
        Commands::Project { command } => dispatch_project(command),
        Commands::Check {
            stage_id,
            suggest,
            args,
        } => run_check(stage_id, suggest, args),
        Commands::SkillIndex => skill_index::execute(),
        Commands::Completions(args) => dispatch_completions(args),
        Commands::Context { command } => dispatch_context(command),
        Commands::Hook { command } => dispatch_hook(command),
        Commands::Complete { shell, args } => dispatch_complete(&shell, &args),
    }
}
