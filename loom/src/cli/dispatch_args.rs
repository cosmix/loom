//! Private, typed arguments for command families routed from the public Clap enum.

use std::path::PathBuf;

use super::types::{Commands, KnowledgeCommands, StatusWebArgs};

/// The caller's variant pattern guards each conversion; an error is a routing bug.
pub(super) fn narrow_family<T, U>(command: U) -> anyhow::Result<T>
where
    T: TryFrom<U>,
{
    T::try_from(command).map_err(|_| anyhow::anyhow!("command family mismatch"))
}

pub(super) enum RuntimeCommand {
    Run {
        manual: bool,
        max_parallel: Option<usize>,
        foreground: bool,
        watch: bool,
        no_merge: bool,
        backend: Option<String>,
        accept_unix_socket_risk: bool,
        daemon_child: Option<PathBuf>,
    },
    Status {
        live: bool,
        compact: bool,
        verbose: bool,
        web_args: StatusWebArgs,
    },
}

impl TryFrom<Commands> for RuntimeCommand {
    type Error = ();

    fn try_from(command: Commands) -> Result<Self, Self::Error> {
        match command {
            Commands::Run {
                manual,
                max_parallel,
                foreground,
                watch,
                no_merge,
                backend,
                accept_unix_socket_risk,
                daemon_child,
            } => Ok(Self::Run {
                manual,
                max_parallel,
                foreground,
                watch,
                no_merge,
                backend,
                accept_unix_socket_risk,
                daemon_child,
            }),
            Commands::Status {
                live,
                compact,
                verbose,
                web_args,
            } => Ok(Self::Status {
                live,
                compact,
                verbose,
                web_args,
            }),
            _ => Err(()),
        }
    }
}

pub(super) struct KnowledgeContextArgs {
    pub stage: Option<String>,
    pub query: String,
    pub budget_tokens: usize,
    pub scope: String,
    pub require_id: Vec<String>,
    pub history: bool,
    pub require_compact: bool,
    pub explain: bool,
    pub json: bool,
}

pub(super) struct CompletionsArgs {
    pub shell: Option<String>,
    pub install: bool,
    pub migrate: bool,
}

impl TryFrom<Commands> for CompletionsArgs {
    type Error = ();

    fn try_from(command: Commands) -> Result<Self, Self::Error> {
        match command {
            Commands::Completions {
                shell,
                install,
                migrate,
            } => Ok(Self {
                shell,
                install,
                migrate,
            }),
            _ => Err(()),
        }
    }
}

impl TryFrom<KnowledgeCommands> for KnowledgeContextArgs {
    type Error = ();

    fn try_from(command: KnowledgeCommands) -> Result<Self, Self::Error> {
        match command {
            KnowledgeCommands::Context {
                stage,
                query,
                budget_tokens,
                scope,
                require_id,
                history,
                require_compact,
                explain,
                json,
            } => Ok(Self {
                stage,
                query,
                budget_tokens,
                scope,
                require_id,
                history,
                require_compact,
                explain,
                json,
            }),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parsed_run_and_status_keep_their_runtime_fields() {
        let run =
            super::super::types::Cli::try_parse_from(["loom", "run", "--foreground", "--watch"])
                .unwrap()
                .command;
        let RuntimeCommand::Run {
            foreground, watch, ..
        } = narrow_family(run).unwrap()
        else {
            panic!("run routed to status");
        };
        assert!(foreground && watch);

        let status = super::super::types::Cli::try_parse_from(["loom", "status", "--compact"])
            .unwrap()
            .command;
        let RuntimeCommand::Status { compact, .. } = narrow_family(status).unwrap() else {
            panic!("status routed to run");
        };
        assert!(compact);
    }
}
