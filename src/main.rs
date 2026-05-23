use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DEFAULT_CONFIG_TOML: &str = r#"
projects_root = "~/repos"
sandbox_home = "~/.agent-sandbox"
"#;

const DEFAULT_SETTINGS_JSON: &str = r#"{
  "network": {
    "allowedDomains": [
      "api.anthropic.com",
      "localhost",
      "127.0.0.1"
    ],
    "deniedDomains": [
      "github.com",
      "*.github.com",
      "gitlab.com",
      "*.gitlab.com",
      "dev.azure.com",
      "*.visualstudio.com",
      "npmjs.org",
      "*.npmjs.org",
      "registry.npmjs.org",
      "pypi.org",
      "files.pythonhosted.org",
      "nuget.org",
      "*.nuget.org",
      "api.nuget.org",
      "crates.io",
      "static.crates.io",
      "index.crates.io",
      "registry-1.docker.io",
      "auth.docker.io",
      "ghcr.io"
    ],
    "allowUnixSockets": [],
    "allowAllUnixSockets": false,
    "allowLocalBinding": true
  },
  "filesystem": {
    "denyRead": [
      "~/.ssh",
      "~/.gnupg",
      "~/.aws",
      "~/.azure",
      "~/.config/gh",
      "~/.config/github-copilot",
      "~/.config/gcloud",
      "~/.config/azure",
      "~/.docker",
      "~/.kube",
      "~/.npmrc",
      "~/.pypirc",
      "~/.netrc",
      "~/.cargo/credentials",
      "~/.cargo/credentials.toml",
      "~/.nuget",
      "~/.m2/settings.xml",
      "~/.gradle/gradle.properties",
      "~/.local/share/opencode/auth.json",
      ".env",
      ".env.local",
      ".envrc",
      ".npmrc",
      ".pypirc",
      "NuGet.config",
      "nuget.config"
    ],
    "allowRead": [],
    "allowWrite": [
      ".",
      "~/.agent-sandbox",
      "/tmp",
      "/dev/shm"
    ],
    "denyWrite": [
      ".env",
      ".env.local",
      ".envrc",
      ".npmrc",
      ".pypirc",
      "NuGet.config",
      "nuget.config",
      ".git/config",
      ".git/hooks",
      ".github/workflows"
    ],
    "allowGitConfig": false
  },
  "ignoreViolations": {},
  "mandatoryDenySearchDepth": 5,
  "enableWeakerNestedSandbox": false,
  "enableWeakerNetworkIsolation": false
}
"#;

#[derive(Debug, Deserialize, Serialize)]
struct WrapperConfig {
    projects_root: String,
    sandbox_home: String,
}

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
        CommandKind::Init => init(),
        CommandKind::Prepare { agent } => {
            prepare_agent(&agent)?;
            Ok(0)
        }
        CommandKind::Run(args) => run(args),
        CommandKind::Pi(args) => run_shortcut("pi", args),
        CommandKind::Opencode(args) => run_shortcut("opencode", args),
        CommandKind::Claude(args) => run_shortcut("claude", args),
        CommandKind::Copilot(args) => run_shortcut("copilot", args),
        CommandKind::Doctor(args) => doctor(args),
    }
}

fn load_config() -> Result<WrapperConfig, Box<dyn std::error::Error>> {
    let config_dir = config_dir()?;
    let config_path = config_dir.join("config.toml");

    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        toml::from_str(&content)?
    } else {
        WrapperConfig {
            projects_root: "~/repos".to_string(),
            sandbox_home: "~/.agent-sandbox".to_string(),
        }
    };

    if let Some(val) = env::var_os("AGENT_SANDBOX_PROJECTS_ROOT") {
        config.projects_root = val.to_string_lossy().to_string();
    }
    if let Some(val) = env::var_os("AGENT_SANDBOX_HOME") {
        config.sandbox_home = val.to_string_lossy().to_string();
    }

    Ok(config)
}

fn resolve_path(path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let expanded = if let Some(rest) = path.strip_prefix('~') {
        let home = home_dir()?;
        if rest.is_empty() || rest == "/" {
            home
        } else {
            let rest = rest.trim_start_matches('/');
            home.join(rest)
        }
    } else {
        PathBuf::from(path)
    };
    if expanded.is_relative() {
        Ok(env::current_dir()?.join(expanded))
    } else {
        Ok(expanded)
    }
}

fn prepare_settings(base_path: &Path, projects_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(base_path)?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(allow_write) = settings
        .pointer_mut("/filesystem/allowWrite")
        .and_then(|v| v.as_array_mut())
    {
        for item in allow_write.iter_mut() {
            if item.as_str() == Some(".") {
                *item = serde_json::Value::String(projects_root.to_string_lossy().to_string());
            }
        }
    }

    let tmp_dir = env::temp_dir().join(format!("agent-sandbox-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir)?;
    let tmp_path = tmp_dir.join("settings.json");
    fs::write(&tmp_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(tmp_path)
}

fn init() -> Result<u8, Box<dyn std::error::Error>> {
    let config_dir = config_dir()?;
    let config_path = config_dir.join("config.toml");
    let settings_path = config_dir.join("settings.json");
    let workspace = resolve_path("~/.agent-sandbox")?;

    fs::create_dir_all(&config_dir)?;
    ensure_workspace_dirs(&workspace)?;

    if !config_path.exists() {
        fs::write(&config_path, DEFAULT_CONFIG_TOML)?;
        println!("created {}", config_path.display());
    } else {
        println!("config already exists: {}", config_path.display());
    }

    if !settings_path.exists() {
        fs::write(&settings_path, DEFAULT_SETTINGS_JSON)?;
        println!("created {}", settings_path.display());
    } else {
        println!("settings already exists: {}", settings_path.display());
    }

    println!("sandbox workspace: {}", workspace.display());
    println!(
        "review settings before first real use: {}",
        settings_path.display()
    );
    Ok(0)
}

fn run_shortcut(agent: &str, shortcut: ShortcutArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let mut command = vec![OsString::from(agent)];
    command.extend(shortcut.args);
    run(RunArgs {
        settings: shortcut.settings,
        workspace: shortcut.workspace,
        projects_root: shortcut.projects_root,
        no_prepare: shortcut.no_prepare,
        command,
    })
}

fn doctor(args: DocArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let command = vec![
        OsString::from("curl"),
        OsString::from("-fsS"),
        OsString::from(&args.daemon_url),
    ];
    run(RunArgs {
        settings: args.settings,
        workspace: args.workspace,
        projects_root: args.projects_root,
        no_prepare: true,
        command,
    })
}

fn run(args: RunArgs) -> Result<u8, Box<dyn std::error::Error>> {
    let config = load_config()?;
    let settings_path = args.settings.map(Ok).unwrap_or_else(settings_path)?;
    let projects_root = args
        .projects_root
        .map(Ok)
        .unwrap_or_else(|| resolve_path(&config.projects_root))?;
    let sandbox_home = args
        .workspace
        .map(Ok)
        .unwrap_or_else(|| resolve_path(&config.sandbox_home))?;

    if !settings_path.exists() {
        return Err(format!(
            "settings file not found: {}. Run `agent-sandbox init` first.",
            settings_path.display()
        )
        .into());
    }

    fs::create_dir_all(&projects_root)?;

    let user_cwd = env::current_dir()?;
    if !user_cwd.starts_with(&projects_root) {
        eprintln!(
            "agent-sandbox: CWD {} is outside projects root {}; changing CWD",
            user_cwd.display(),
            projects_root.display()
        );
        env::set_current_dir(&projects_root)?;
    }

    let dynamic_settings = prepare_settings(&settings_path, &projects_root)?;

    ensure_workspace_dirs(&sandbox_home)?;
    configure_agent_runtime(&sandbox_home, &args.command[0])?;

    let command_name = command_name(&args.command[0]);
    if !args.no_prepare && which(&command_name).is_none() {
        if let Some(agent) = known_agent_for_command(&command_name) {
            eprintln!("agent-sandbox: preparing missing agent command `{command_name}` on host");
            prepare_agent(agent)?;
        }
    }

    let status = Command::new("srt")
        .arg("--settings")
        .arg(&dynamic_settings)
        .arg("--")
        .args(args.command)
        .env("HOME", sandbox_home.join("home"))
        .env("XDG_CONFIG_HOME", sandbox_home.join("config"))
        .env("XDG_CACHE_HOME", sandbox_home.join("cache"))
        .env("XDG_DATA_HOME", sandbox_home.join("share"))
        .env("TMPDIR", sandbox_home.join("tmp"))
        .env("npm_config_cache", sandbox_home.join("npm-cache"))
        .env("npm_config_prefix", sandbox_home.join("npm-prefix"))
        .env("npm_config_audit", "false")
        .env("npm_config_fund", "false")
        .env("npm_config_update_notifier", "false")
        .status()?;

    Ok(exit_code(status.code()))
}

fn configure_agent_runtime(workspace: &Path, command: &OsStr) -> io::Result<()> {
    match command_name(command).as_str() {
        "opencode" => {
            let cfg = workspace.join("config/opencode");
            fs::create_dir_all(&cfg)?;
            write_if_missing(&cfg.join("opencode.json"), "{}\n")?;
            write_if_missing(&cfg.join("tui.json"), "{}\n")?;
        }
        "pi" | "pi-agent" => {
            let pi_dir = workspace.join("home/.pi/agent");
            fs::create_dir_all(&pi_dir)?;
            let deny_npm = workspace.join("bin/agent-sandbox-npm-deny");
            write_if_missing(
                &deny_npm,
                "#!/usr/bin/env bash\nprintf 'agent-sandbox: npm is disabled inside the sandbox; run host preparation or use the future helper daemon.\\n' >&2\nexit 126\n",
            )?;
            make_executable(&deny_npm)?;
            let settings = format!("{{\n  \"npmCommand\": [\"{}\"]\n}}\n", deny_npm.display());
            write_if_missing(&pi_dir.join("settings.json"), &settings)?;
        }
        _ => {}
    }

    Ok(())
}

fn prepare_agent(agent: &str) -> Result<(), Box<dyn std::error::Error>> {
    match agent {
        "opencode" => npm_install_global("opencode-ai")?,
        "pi" | "pi-agent" => npm_install_global("@mariozechner/pi-coding-agent")?,
        "claude" => npm_install_global("@anthropic-ai/claude-code")?,
        other => return Err(format!("no preparation recipe for `{other}`").into()),
    }
    Ok(())
}

fn npm_install_global(package: &str) -> io::Result<()> {
    let npm = env::var_os("AGENT_SANDBOX_NPM").unwrap_or_else(|| OsString::from("npm"));
    let status = Command::new(npm)
        .arg("install")
        .arg("-g")
        .arg(package)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("npm install -g {package} failed"),
        ))
    }
}

fn known_agent_for_command(command: &str) -> Option<&'static str> {
    match command {
        "opencode" => Some("opencode"),
        "pi" | "pi-agent" => Some("pi"),
        "claude" => Some("claude"),
        _ => None,
    }
}

fn ensure_workspace_dirs(root: &Path) -> io::Result<()> {
    for name in [
        "home",
        "config",
        "cache",
        "share",
        "tmp",
        "npm-cache",
        "npm-prefix",
        "bin",
        "logs",
    ] {
        fs::create_dir_all(root.join(name))?;
    }
    Ok(())
}

fn write_if_missing(path: &Path, content: &str) -> io::Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("agent-sandbox"));
    }
    Ok(home_dir()?.join(".config/agent-sandbox"))
}

fn settings_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = env::var_os("AGENT_SANDBOX_SETTINGS") {
        return Ok(PathBuf::from(path));
    }
    Ok(config_dir()?.join("settings.json"))
}

fn home_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn which(command: &str) -> Option<PathBuf> {
    if command.contains('/') {
        let path = PathBuf::from(command);
        return path.exists().then_some(path);
    }

    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn command_name(command: &OsStr) -> String {
    Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

fn exit_code(code: Option<i32>) -> u8 {
    code.unwrap_or(1).try_into().unwrap_or(1)
}
