//! `loom ask`: answer one question about named files with a one-shot worker
//! that has no tools, no settings and no project instructions, or score a
//! backend against a cases file with `--eval`.

mod backend;
mod eval;
mod payload;
mod run;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;

use crate::fs::work_dir::WorkDir;
use payload::Document;

pub use backend::Backend;
pub(crate) use backend::WorkerReply;

/// Exit code after SIGINT or SIGTERM.
const EXIT_INTERRUPTED: i32 = 130;

/// Largest `--timeout-secs`: a day. A larger `Duration` overflows the
/// deadline arithmetic in the process runner.
const MAX_TIMEOUT_SECS: u64 = 86_400;

/// Arguments for `loom ask`.
#[derive(Debug, clap::Parser)]
#[command(group(clap::ArgGroup::new("mode").required(true).args(["question", "eval"])))]
pub struct AskArgs {
    /// The question to answer about the files
    #[arg(long, value_name = "Q", requires = "paths")]
    pub question: Option<String>,
    /// Files the question is about, inside the project (400,000 bytes in total)
    #[arg(long, value_name = "P", num_args = 1.., requires = "question", conflicts_with = "eval")]
    pub paths: Vec<PathBuf>,
    /// Score the backend against a JSON array of cases
    #[arg(long, value_name = "CASES")]
    pub eval: Option<PathBuf>,
    /// Provider CLI that answers
    #[arg(long, value_enum, default_value_t = Backend::Claude)]
    pub backend: Backend,
    /// Model (default: haiku on claude, the luna tier on codex)
    #[arg(long, value_name = "M")]
    pub model: Option<String>,
    /// Seconds one worker may run (1 to 86400)
    #[arg(long, value_name = "N", default_value_t = 180,
          value_parser = clap::value_parser!(u64).range(1..=MAX_TIMEOUT_SECS))]
    pub timeout_secs: u64,
    /// Print one JSON object instead of the answer and a usage line
    #[arg(long, conflicts_with = "eval")]
    pub json: bool,
}

/// One question for one worker.
pub(crate) struct AskRequest {
    pub question: String,
    pub backend: Backend,
    pub model: Option<String>,
    pub timeout: Duration,
}

impl AskRequest {
    fn model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or_else(|| self.backend.default_model())
    }
}

/// Run `loom ask`. A refusal or a worker failure is an error (exit 1); a
/// signal exits 130; `--eval` exits with its own verdict.
pub fn execute(args: AskArgs) -> Result<()> {
    let root = project_root()?;
    run::install_interrupt_handler()?;
    let outcome = if args.eval.is_some() {
        eval::run(&root, &args)
    } else {
        answer(&root, &args).map(|()| 0)
    };
    if run::interrupted() {
        eprintln!("loom ask: interrupted");
        std::process::exit(EXIT_INTERRUPTED);
    }
    match outcome? {
        0 => Ok(()),
        code => std::process::exit(code),
    }
}

/// The nearest workspace's project root, else the current directory (never
/// the enclosing git root), canonicalized once.
fn project_root() -> Result<PathBuf> {
    let work_dir = WorkDir::new(".")?;
    let root = work_dir
        .project_root()
        .context("cannot locate the project root")?;
    root.canonicalize()
        .with_context(|| format!("cannot resolve the project root {}", root.display()))
}

/// Ask one question about `paths` under `project_root`, which must be
/// canonical.
pub(crate) fn ask(
    project_root: &Path,
    paths: &[PathBuf],
    request: &AskRequest,
) -> Result<WorkerReply> {
    let documents = payload::read_documents(project_root, paths)?;
    ask_documents(&documents, request)
}

/// Ask about documents already read; `--eval` counts their characters first.
fn ask_documents(documents: &[Document], request: &AskRequest) -> Result<WorkerReply> {
    let backend = request.backend;
    let payload = payload::assemble(backend.stdin_lead(), documents, &request.question);
    let program = backend.binary()?;
    let finished = run::run_worker(&run::Worker {
        program: &program,
        args: &backend.args(request.model()),
        env: backend.child_env(),
        payload: &payload,
        timeout: request.timeout,
    })?;
    let mut reply = backend.parse(&finished.stdout)?;
    reply.latency_ms = finished.latency_ms;
    Ok(reply)
}

fn answer(root: &Path, args: &AskArgs) -> Result<()> {
    let request = AskRequest {
        question: args.question.clone().context("--question is required")?,
        backend: args.backend,
        model: args.model.clone(),
        timeout: Duration::from_secs(args.timeout_secs),
    };
    let reply = ask(root, &args.paths, &request)?;
    let (backend, model) = (request.backend.name(), request.model());
    if args.json {
        let report = json!({
            "backend": backend,
            "model": model,
            "answer": reply.answer,
            "input_tokens": reply.input_tokens,
            "cache_read_tokens": reply.cache_read_tokens,
            "cache_write_tokens": reply.cache_write_tokens,
            "output_tokens": reply.output_tokens,
            "latency_ms": reply.latency_ms,
        });
        println!("{report}");
    } else {
        println!("{}", reply.answer);
        eprintln!(
            "{backend} {}: input {} cache_read {} cache_write {} output {} tokens, {} ms",
            crate::context::untrusted::inline_safe(model),
            reply.input_tokens,
            reply.cache_read_tokens,
            reply.cache_write_tokens,
            reply.output_tokens,
            reply.latency_ms
        );
    }
    Ok(())
}
