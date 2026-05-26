use anyhow::{anyhow, Result};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config;
use crate::policy;

pub fn run(
    workspace_arg: Option<PathBuf>,
    projects_root_arg: Option<PathBuf>,
    extra_write_dirs: Vec<PathBuf>,
    command: Vec<OsString>,
) -> Result<u8> {
    let cfg = config::load_config()?;
    let projects_root = projects_root_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.projects_root))?;
    let sandbox_home = workspace_arg
        .map(Ok)
        .unwrap_or_else(|| config::resolve_path(&cfg.sandbox_home))?;
    let host_home = config::home_dir()?;

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

    let mut all_extra_dirs: Vec<PathBuf> = Vec::new();
    for d in &cfg.extra_write_dirs {
        all_extra_dirs.push(config::resolve_path(d)?);
    }
    for d in &extra_write_dirs {
        all_extra_dirs.push(config::resolve_path(&d.to_string_lossy())?);
    }

    let daemon_sock = sandbox_home.join("daemon.sock");
    ensure_daemon_running(&daemon_sock, &projects_root)?;

    let dynamic_settings = policy::prepare_settings(
        &projects_root,
        &daemon_sock,
        &cfg.network.allowed_domains,
        &all_extra_dirs,
    )?;

    ensure_workspace_dirs(&sandbox_home)?;

    let bin_dir = sandbox_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
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

    let cmd_name = command_name(&command[0]);

    let allowed = policy::discover_allow_read_paths(&host_home);
    let current_path = env::var_os("PATH").unwrap_or_default();
    let sandbox_path = env::join_paths(std::iter::once(sandbox_home.join("bin")).chain(
        env::split_paths(&current_path).filter(|p| {
            if p.starts_with(&host_home) {
                allowed.iter().any(|a| p.starts_with(a))
            } else {
                true
            }
        }),
    ))?;

    let mut daemonized: Vec<OsString> = Vec::new();
    daemonized.push(OsString::from("env"));
    daemonized.push(OsString::from(format!(
        "HELPER_DAEMON_SOCK={}",
        daemon_sock.display()
    )));
    daemonized.push(OsString::from(format!(
        "PATH={}",
        sandbox_path.to_string_lossy()
    )));
    daemonized.extend(command);

    let mut cmd = Command::new("srt");
    cmd.arg("--settings").arg(dynamic_settings.as_os_str());
    cmd.arg("--");
    for arg in daemonized {
        cmd.arg(arg);
    }
    cmd.env("XDG_CACHE_HOME", sandbox_home.join("cache"))
        .env("XDG_DATA_HOME", sandbox_home.join("share"))
        .env("XDG_STATE_HOME", sandbox_home.join("state"));

    let host_cargo = host_home.join(".cargo");
    if host_cargo.exists() {
        cmd.env("CARGO_HOME", host_cargo);
    }

    let host_rustup = host_home.join(".rustup");
    if host_rustup.exists() {
        cmd.env("RUSTUP_HOME", host_rustup);
    }

    let host_nuget = host_home.join(".nuget/packages");
    if host_nuget.exists() {
        cmd.env("NUGET_PACKAGES", host_nuget);
    }

    for (key, val) in host_git_identity() {
        cmd.env(key, val);
    }

    if cmd_name == "pi" || cmd_name == "pi-agent" {
        cmd.env("PI_OFFLINE", "true");
    } else if cmd_name == "opencode" {
        cmd.env("OPENCODE_DISABLE_AUTOUPDATE", "true");
    }

    let mut child = cmd.spawn().unwrap_or_else(|e| {
        eprintln!("agent-sandbox: failed to spawn srt: {e}");
        std::process::exit(127);
    });

    let status = child.wait().unwrap_or_else(|e| {
        eprintln!("agent-sandbox: failed to wait for srt: {e}");
        std::process::exit(1);
    });

    Ok(status.code().unwrap_or(1) as u8)
}

fn sibling_binary(name: &str) -> Result<PathBuf> {
    let exe = env::current_exe()?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow!("cannot determine path of current executable"))?;
    let candidate = parent.join(name);
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "binary not found next to this executable: expected {}",
            candidate.display()
        ))
    }
}

fn healthz_check(conn: &mut UnixStream) -> bool {
    if writeln!(conn, "healthz").is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let n = match conn.read(&mut buf) {
        Ok(0) | Err(_) => return false,
        Ok(n) => n,
    };
    serde_json::from_slice::<serde_json::Value>(&buf[..n])
        .ok()
        .and_then(|v| v.get("ok")?.as_bool())
        == Some(true)
}

fn ensure_daemon_running(socket_path: &Path, projects_root: &Path) -> Result<()> {
    if let Ok(mut conn) = UnixStream::connect(socket_path) {
        if healthz_check(&mut conn) {
            return Ok(());
        }
    }

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut child = Command::new(sibling_binary("agent-sandbox-helper-daemon")?)
        .arg("--socket-path")
        .arg(socket_path)
        .arg("--projects-root")
        .arg(projects_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    for _ in 0..50 {
        if let Ok(mut conn) = UnixStream::connect(socket_path) {
            if healthz_check(&mut conn) {
                return Ok(());
            }
        }
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("daemon exited prematurely with {status}"));
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(anyhow!("daemon socket did not appear within 5s"))
}

pub fn ensure_workspace_dirs(root: &Path) -> io::Result<()> {
    for name in ["cache", "share", "state", "bin"] {
        fs::create_dir_all(root.join(name))?;
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

fn host_git_identity() -> Vec<(String, String)> {
    let mut vars = Vec::new();
    for (key, suffix) in [("user.name", "NAME"), ("user.email", "EMAIL")] {
        if let Ok(output) = Command::new("git")
            .args(["config", "--global", key])
            .output()
        {
            let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !val.is_empty() && output.status.success() {
                vars.push((format!("GIT_AUTHOR_{suffix}"), val.clone()));
                vars.push((format!("GIT_COMMITTER_{suffix}"), val));
            }
        }
    }
    vars
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
}
