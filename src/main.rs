mod agent;
mod config;
mod policy;
mod sandbox;

use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox")]
#[command(about = "Convenience wrapper around srt for sandboxed coding agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Create default config and runtime directories.
    Init,
    /// Prepare a known agent in the sandbox home.
    Prepare {
        /// Known agent name: pi, or opencode (also accepts pi-agent).
        agent: String,
        /// Sandbox-visible runtime state root. Defaults to the value in config.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Run any command inside the sandbox.
    Run(RunArgs),
    /// Shortcut for `agent-sandbox run -- pi ...`.
    Pi(ShortcutArgs),
    /// Shortcut for `agent-sandbox run -- opencode ...`.
    Opencode(ShortcutArgs),
    /// Run a connectivity health check inside the sandbox against the helper daemon.
    Healthcheck(HealthcheckArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Do not auto-install known missing agent commands.
    #[arg(long)]
    no_prepare: bool,
    /// Additional host directories the sandbox can write to (repeatable).
    #[arg(long)]
    allow_write: Vec<PathBuf>,
    /// Command and arguments to run after `--`.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<OsString>,
}

#[derive(Debug, Args)]
struct ShortcutArgs {
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Do not auto-install the agent if the command is missing.
    #[arg(long)]
    no_prepare: bool,
    /// Additional host directories the sandbox can write to (repeatable).
    #[arg(long)]
    allow_write: Vec<PathBuf>,
    /// Arguments passed to the agent.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

#[derive(Debug, Args)]
struct HealthcheckArgs {
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Additional host directories the sandbox can write to (repeatable).
    #[arg(long)]
    allow_write: Vec<PathBuf>,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("agent-sandbox: {err}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<u8, Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        CommandKind::Init => config::init(),
        CommandKind::Prepare { agent, workspace } => {
            let sandbox_home = match workspace {
                Some(p) => p,
                None => {
                    let cfg = config::load_config()?;
                    config::resolve_path(&cfg.sandbox_home)?
                }
            };
            agent::prepare(&agent, &sandbox_home)?;
            Ok(0)
        }
        CommandKind::Run(args) => sandbox::run(
            args.workspace,
            args.projects_root,
            args.no_prepare,
            args.allow_write,
            args.command,
        ),
        CommandKind::Pi(args) => run_shortcut("pi", args),
        CommandKind::Opencode(args) => run_shortcut("opencode", args),
        CommandKind::Healthcheck(args) => healthcheck(args),
    }
}

fn run_shortcut(agent: &str, shortcut: ShortcutArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let mut command = vec![OsString::from(agent)];
    command.extend(shortcut.args);
    sandbox::run(
        shortcut.workspace,
        shortcut.projects_root,
        shortcut.no_prepare,
        shortcut.allow_write,
        command,
    )
}

fn healthcheck(args: HealthcheckArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let command = vec![
        OsString::from("agent-sandbox-helper"),
        OsString::from("healthz"),
    ];
    sandbox::run(
        args.workspace,
        args.projects_root,
        true,
        args.allow_write,
        command,
    )
}
