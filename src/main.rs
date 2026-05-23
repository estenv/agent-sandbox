mod agent;
mod config;
mod sandbox;
mod policy;

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
    /// Prepare a known agent on the host, outside the sandbox.
    Prepare {
        /// Known agent name: pi, opencode, or claude.
        agent: String,
    },
    /// Run any command inside the sandbox.
    Run(RunArgs),
    /// Shortcut for `agent-sandbox run -- pi ...`.
    Pi(ShortcutArgs),
    /// Shortcut for `agent-sandbox run -- opencode ...`.
    Opencode(ShortcutArgs),
    /// Shortcut for `agent-sandbox run -- claude ...`.
    Claude(ShortcutArgs),
    /// Shortcut for `agent-sandbox run -- copilot ...`.
    Copilot(ShortcutArgs),
    /// Run a quick connectivity check against the helper daemon from inside srt.
    Doctor(DocArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// SRT settings file. Defaults to ~/.config/agent-sandbox/settings.json.
    #[arg(long)]
    settings: Option<PathBuf>,
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Do not auto-install known missing agent commands.
    #[arg(long)]
    no_prepare: bool,
    /// Command and arguments to run after `--`.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<OsString>,
}

#[derive(Debug, Args)]
struct ShortcutArgs {
    /// SRT settings file. Defaults to ~/.config/agent-sandbox/settings.json.
    #[arg(long)]
    settings: Option<PathBuf>,
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Do not auto-install the agent if the command is missing.
    #[arg(long)]
    no_prepare: bool,
    /// Arguments passed to the agent.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

#[derive(Debug, Args)]
struct DocArgs {
    /// SRT settings file. Defaults to ~/.config/agent-sandbox/settings.json.
    #[arg(long)]
    settings: Option<PathBuf>,
    /// Sandbox-visible runtime state root. Defaults to ~/.agent-sandbox.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Projects root directory. Defaults to the value in config.toml.
    #[arg(long)]
    projects_root: Option<PathBuf>,
    /// Helper daemon URL to test.
    #[arg(long, default_value = "http://localhost:47688/healthz")]
    daemon_url: String,
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
        CommandKind::Prepare { agent } => {
            agent::prepare(&agent)?;
            Ok(0)
        }
        CommandKind::Run(args) => sandbox::run(
            args.settings,
            args.workspace,
            args.projects_root,
            args.no_prepare,
            args.command,
        ),
        CommandKind::Pi(args) => run_shortcut("pi", args),
        CommandKind::Opencode(args) => run_shortcut("opencode", args),
        CommandKind::Claude(args) => run_shortcut("claude", args),
        CommandKind::Copilot(args) => run_shortcut("copilot", args),
        CommandKind::Doctor(args) => doctor(args),
    }
}

fn run_shortcut(agent: &str, shortcut: ShortcutArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let mut command = vec![OsString::from(agent)];
    command.extend(shortcut.args);
    sandbox::run(
        shortcut.settings,
        shortcut.workspace,
        shortcut.projects_root,
        shortcut.no_prepare,
        command,
    )
}

fn doctor(args: DocArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let command = vec![
        OsString::from("curl"),
        OsString::from("-fsS"),
        OsString::from(&args.daemon_url),
    ];
    sandbox::run(
        args.settings,
        args.workspace,
        args.projects_root,
        true,
        command,
    )
}
