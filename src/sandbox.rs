use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::agent;
use crate::config;
use crate::policy;

pub fn run(
    settings_arg: Option<PathBuf>,
    workspace_arg: Option<PathBuf>,
    projects_root_arg: Option<PathBuf>,
    no_prepare: bool,
    command: Vec<OsString>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let settings_path = settings_arg
        .map(Ok)
        .unwrap_or_else(config::settings_path)?;
    let projects_root = projects_root_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.projects_root))?;
    let sandbox_home = workspace_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.sandbox_home))?;

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

    let dynamic_settings = policy::prepare_settings(&settings_path, &projects_root)?;

    ensure_workspace_dirs(&sandbox_home)?;
    configure_agent_runtime(&sandbox_home, &command[0])?;

    let cmd_name = command_name(&command[0]);
    if !no_prepare && which(&cmd_name).is_none() {
        if let Some(agent_name) = agent::known_for_command(&cmd_name) {
            eprintln!("agent-sandbox: preparing missing agent command `{cmd_name}` on host");
            agent::prepare(agent_name)?;
        }
    }

    let status = Command::new("srt")
        .arg("--settings")
        .arg(&dynamic_settings)
        .arg("--")
        .args(&command)
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

pub fn ensure_workspace_dirs(root: &Path) -> io::Result<()> {
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
            let pi_settings = format!("{{\n  \"npmCommand\": [\"{}\"]\n}}\n", deny_npm.display());
            write_if_missing(&pi_dir.join("settings.json"), &pi_settings)?;
        }
        _ => {}
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
