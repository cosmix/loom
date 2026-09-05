use clap::Args;

/// Dashboard-only options kept separate to preserve the main CLI type's size budget.
#[derive(Args)]
pub struct StatusWebArgs {
    /// Serve the live dashboard on 127.0.0.1 (PORT defaults to 7373; 0 picks a free port)
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "7373",
        conflicts_with_all = ["live", "compact", "verbose"]
    )]
    pub web: Option<u16>,
}
