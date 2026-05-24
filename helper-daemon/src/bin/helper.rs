use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

use clap::Parser;

use agent_sandbox_helper_daemon::protocol::DaemonCommand;

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox-helper")]
#[command(about = "Send commands to the agent-sandbox helper daemon")]
#[command(version)]
struct Cli {
    /// Path to the daemon Unix socket.
    #[arg(long, env = "HELPER_DAEMON_SOCK")]
    socket_path: String,

    #[command(subcommand)]
    command: HelperCommand,
}

#[derive(Debug, clap::Subcommand)]
enum HelperCommand {
    /// Check if the daemon is running
    Healthz,
    /// Test connectivity to the daemon
    Test,
    /// Pull latest changes in a git repository
    GitPull {
        /// Absolute path to the git repository
        path: String,
    },
    /// Push changes in a git repository (blocked on main/master)
    GitPush {
        /// Absolute path to the git repository
        path: String,
    },
    /// Create a pull request in Azure DevOps
    PrCreate {
        /// Absolute path to the git repository
        #[arg(long)]
        path: String,
        /// PR title
        #[arg(long)]
        title: String,
        /// Source branch
        #[arg(long)]
        source: String,
        /// Target branch (defaults to repository default)
        #[arg(long)]
        target: Option<String>,
        /// PR description
        #[arg(long)]
        description: Option<String>,
    },
    /// Install dependencies (detects package manager from lockfile)
    DepInstall {
        /// Absolute path to the project directory
        path: String,
    },
    /// List work items assigned to me (uses WIQL via ado cli)
    WiList {
        /// Absolute path to the git repository (to detect provider)
        path: String,
    },
    /// Create work item (task by default); optional parent link + desc
    WiCreate {
        /// Absolute path to git repo (provider detect)
        #[arg(long)]
        path: String,
        /// Work item title (required)
        #[arg(long)]
        title: String,
        /// Parent work item ID to link under
        #[arg(long)]
        parent: Option<i64>,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
        /// Work item type (defaults to Task)
        #[arg(long)]
        r#type: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let cmd = match cli.command {
        HelperCommand::Healthz => DaemonCommand::Healthz,
        HelperCommand::Test => DaemonCommand::Test,
        HelperCommand::GitPull { path } => DaemonCommand::GitPull { path },
        HelperCommand::GitPush { path } => DaemonCommand::GitPush { path },
        HelperCommand::PrCreate {
            path,
            title,
            source,
            target,
            description,
        } => DaemonCommand::PrCreate {
            path,
            title,
            source,
            target,
            description,
        },
        HelperCommand::DepInstall { path } => DaemonCommand::DepInstall { path },
        HelperCommand::WiList { path } => DaemonCommand::WiList { path },
        HelperCommand::WiCreate {
            path,
            title,
            parent,
            description,
            r#type,
        } => DaemonCommand::WiCreate {
            path,
            title,
            parent,
            description,
            r#type,
        },
    };

    let wire = cmd.to_wire();

    let mut conn = match UnixStream::connect(&cli.socket_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to connect to daemon socket: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = writeln!(conn, "{wire}") {
        eprintln!("error: failed to send request: {e}");
        return ExitCode::from(1);
    }

    let mut response = Vec::new();
    if let Err(e) = conn.read_to_end(&mut response) {
        eprintln!("error: failed to read response: {e}");
        return ExitCode::from(1);
    }

    if let Err(e) = std::io::stdout().write_all(&response) {
        eprintln!("error: failed to write response to stdout: {e}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
