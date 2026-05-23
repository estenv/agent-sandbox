use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::agent;
use crate::config;
use crate::policy;

pub fn run(
    workspace_arg: Option<PathBuf>,
    projects_root_arg: Option<PathBuf>,
    no_prepare: bool,
    command: Vec<OsString>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let projects_root = projects_root_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.projects_root))?;
    let sandbox_home = workspace_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.sandbox_home))?;

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

    let daemon_sock = sandbox_home.join("daemon.sock");
    ensure_daemon_running(&daemon_sock, &projects_root)?;

    let dynamic_settings =
        policy::prepare_settings(&projects_root, &daemon_sock, &cfg.network.allowed_domains)?;

    ensure_workspace_dirs(&sandbox_home)?;
    configure_agent_runtime(&sandbox_home, &command[0])?;

    let cmd_name = command_name(&command[0]);
    if !no_prepare && which(&cmd_name).is_none() {
        if let Some(agent_name) = agent::known_for_command(&cmd_name) {
            eprintln!("agent-sandbox: preparing missing agent command `{cmd_name}` on host");
            agent::prepare(agent_name)?;
        }
    }

    let mut daemonized: Vec<OsString> = Vec::new();
    daemonized.push(OsString::from("env"));
    daemonized.push(OsString::from(format!(
        "HELPER_DAEMON_SOCK={}",
        daemon_sock.display()
    )));
    let current_path = env::var_os("PATH").unwrap_or_default();
    daemonized.push(OsString::from(format!(
        "PATH={}/bin:{}",
        sandbox_home.display(),
        current_path.to_string_lossy()
    )));
    daemonized.extend(command);

    let args: Vec<OsString> = vec![
        OsString::from("srt"),
        OsString::from("--settings"),
        dynamic_settings.clone().into(),
        OsString::from("--"),
    ]
    .into_iter()
    .chain(daemonized)
    .collect();

    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Parent { child }) => {
            let status = nix::sys::wait::waitpid(child, None)?;
            Ok(match status {
                nix::sys::wait::WaitStatus::Exited(_, code) => code.try_into().unwrap_or(1),
                nix::sys::wait::WaitStatus::Signaled(_, sig, _) => 128 + sig as u8,
                _ => 1,
            })
        }
        Ok(nix::unistd::ForkResult::Child) => {
            let mut cmd = Command::new(&args[0]);
            for arg in &args[1..] {
                cmd.arg(arg);
            }
            cmd.env("HOME", sandbox_home.join("home"))
                .env("XDG_CONFIG_HOME", sandbox_home.join("config"))
                .env("XDG_CACHE_HOME", sandbox_home.join("cache"))
                .env("XDG_DATA_HOME", sandbox_home.join("share"))
                .env("TMPDIR", sandbox_home.join("tmp"))
                .env("npm_config_cache", sandbox_home.join("npm-cache"))
                .env("npm_config_prefix", sandbox_home.join("npm-prefix"))
                .env("npm_config_audit", "false")
                .env("npm_config_fund", "false")
                .env("npm_config_update_notifier", "false");
            let _err = cmd.exec();
            // exec failed
            eprintln!("agent-sandbox: failed to exec srt: {_err}");
            std::process::exit(127);
        }
        Err(_) => Err("fork failed".into()),
    }
}

fn sibling_binary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe = env::current_exe()?;
    let parent = exe
        .parent()
        .ok_or("cannot determine path of current executable")?;
    let candidate = parent.join(name);
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(format!(
            "binary not found next to this executable: expected {}",
            candidate.display()
        )
        .into())
    }
}

fn ensure_daemon_running(
    socket_path: &Path,
    projects_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Already running?
    if let Ok(mut conn) = UnixStream::connect(socket_path) {
        let _ = writeln!(conn, "healthz");
        return Ok(());
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Spawn daemon (must live next to the agent-sandbox binary)
    let mut child = Command::new(sibling_binary("agent-sandbox-helper-daemon")?)
        .arg("--socket-path")
        .arg(socket_path)
        .arg("--projects-root")
        .arg(projects_root)
        .spawn()?;

    // Wait for socket to appear (poll 5s), verify liveness via health check
    for _ in 0..50 {
        if let Ok(mut conn) = UnixStream::connect(socket_path) {
            let _ = writeln!(conn, "healthz");
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("daemon exited prematurely with {status}").into());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err("daemon socket did not appear within 5s".into())
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
    let bin_dir = workspace.join("bin");
    fs::create_dir_all(&bin_dir)?;

    // Copy the helper binary into the sandbox
    if let Ok(helper_src) = sibling_binary("agent-sandbox-helper") {
        let helper_dst = bin_dir.join("agent-sandbox-helper");
        let _ = fs::remove_file(&helper_dst);
        fs::copy(&helper_src, &helper_dst)?;
        make_executable(&helper_dst)?;
    } else {
        eprintln!(
            "agent-sandbox: warning: helper binary not found — git-pull inside sandbox will fail"
        );
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn test_command_name_simple() {
        assert_eq!(command_name(&OsString::from("opencode")), "opencode");
    }

    #[test]
    fn test_command_name_path_uses_basename() {
        assert_eq!(command_name(&OsString::from("/usr/local/bin/node")), "node");
    }

    #[test]
    fn test_command_name_empty() {
        assert_eq!(command_name(&OsString::from("")), "");
    }

    #[test]
    fn test_command_name_trailing_slash() {
        assert_eq!(command_name(&OsString::from("/usr/bin/")), "bin");
    }

    #[test]
    fn test_command_name_dot_slash() {
        assert_eq!(command_name(&OsString::from("./foo")), "foo");
    }

    #[test]
    fn test_which_absolute_path_exists() {
        assert!(which("/bin/sh").is_some());
    }

    #[test]
    fn test_which_absolute_path_missing() {
        assert!(which("/nonexistent-binary-hopefully").is_none());
    }

    #[test]
    fn test_which_searches_path() {
        assert!(which("sh").is_some());
    }

    #[test]
    fn test_which_unknown_not_found() {
        assert!(which("this-command-should-not-exist-xyzzy").is_none());
    }
}
