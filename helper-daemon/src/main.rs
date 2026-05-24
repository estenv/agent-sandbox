mod ado;
mod cmd;
mod deps;
mod git;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;

use agent_sandbox_helper_daemon::protocol;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox-helper-daemon")]
#[command(about = "Host helper daemon for agent-sandbox — listens on a Unix socket")]
#[command(version)]
struct Cli {
    /// Path to the Unix domain socket.
    #[arg(long, default_value = "~/.agent-sandbox/daemon.sock")]
    socket_path: String,

    /// Restrict file operations to this root directory.
    #[arg(long)]
    projects_root: Option<String>,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("agent-sandbox-helper-daemon: {err}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let socket_path = resolve_path(&cli.socket_path);
    let projects_root = cli.projects_root.as_deref().map(resolve_path);

    // Check if another daemon is already running
    if let Ok(mut conn) = UnixStream::connect(&socket_path) {
        let _ = writeln!(conn, "healthz");
        let mut buf = [0u8; 256];
        if let Ok(n) = conn.read(&mut buf) {
            let resp = String::from_utf8_lossy(&buf[..n]);
            if resp.contains("\"ok\":true") {
                eprintln!("daemon already running on {}", socket_path.display());
                std::process::exit(0);
            }
        }
    }

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let _ = fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    eprintln!(
        "agent-sandbox-helper-daemon listening on {}",
        socket_path.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let root = projects_root.clone();
                thread::spawn(move || {
                    if let Err(ref err) = handle_connection(stream, root.as_deref()) {
                        if err.kind() != std::io::ErrorKind::BrokenPipe {
                            eprintln!("request failed: {err}");
                        }
                    }
                });
            }
            Err(err) => eprintln!("accept failed: {err}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: UnixStream, projects_root: Option<&Path>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let body = handle_request(line.trim(), projects_root);
    let mut stream = reader.into_inner();
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

pub(crate) fn ok_response(data: serde_json::Value) -> String {
    let mut resp = serde_json::json!({"ok": true});
    if let Some(obj) = resp.as_object_mut() {
        if let serde_json::Value::Object(extra) = data {
            obj.extend(extra);
        }
    }
    resp.to_string()
}

pub(crate) fn err_response(error: impl Into<String>) -> String {
    serde_json::json!({"ok": false, "error": error.into()}).to_string()
}

pub(crate) fn validate_path(
    path_str: &str,
    projects_root: Option<&Path>,
) -> Result<PathBuf, String> {
    let cwd = std::fs::canonicalize(path_str).map_err(|e| format!("path does not resolve: {e}"))?;
    if let Some(root) = projects_root {
        let root = std::fs::canonicalize(root)
            .map_err(|e| format!("projects_root does not resolve: {e}"))?;
        if !cwd.starts_with(&root) {
            return Err(format!(
                "path is outside allowed projects root: {}",
                cwd.display()
            ));
        }
    }
    Ok(cwd)
}

pub fn handle_request(line: &str, projects_root: Option<&Path>) -> String {
    let cmd = match protocol::DaemonCommand::from_wire(line) {
        Ok(c) => c,
        Err(e) => return err_response(e),
    };

    match cmd {
        protocol::DaemonCommand::Healthz => {
            ok_response(serde_json::json!({"service": "agent-sandbox-helper-daemon"}))
        }
        protocol::DaemonCommand::Test => {
            ok_response(serde_json::json!({"message": "helper daemon connectivity works"}))
        }
        protocol::DaemonCommand::GitPull { path } => match validate_path(&path, projects_root) {
            Ok(cwd) => git::git_pull(&cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::GitPush { path } => match validate_path(&path, projects_root) {
            Ok(cwd) => git::git_push(&cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::PrCreate {
            path,
            title,
            source,
            target,
            description,
            work_item,
        } => match validate_path(&path, projects_root) {
            Ok(cwd) => {
                let params = ado::PrParams {
                    title,
                    source,
                    target,
                    description,
                    work_item,
                };
                ado::pr_create(&params, &cwd)
            }
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::DepInstall { path } => match validate_path(&path, projects_root) {
            Ok(cwd) => deps::dep_install(&cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::WiList { path } => match validate_path(&path, projects_root) {
            Ok(cwd) => ado::wi_list(&cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::WiCreate {
            path,
            title,
            parent,
            description,
            r#type,
        } => match validate_path(&path, projects_root) {
            Ok(cwd) => ado::wi_create(
                &cwd,
                &title,
                parent,
                description.as_deref(),
                r#type.as_deref(),
            ),
            Err(e) => err_response(e),
        },
    }
}

fn resolve_path(path: &str) -> PathBuf {
    agent_sandbox::config::resolve_path(path).unwrap_or_else(|_| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_healthz() {
        let body = handle_request("healthz", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"].as_bool(), Some(true));
        assert_eq!(
            v["service"].as_str().unwrap(),
            "agent-sandbox-helper-daemon"
        );
    }

    #[test]
    fn test_handle_test_action() {
        let body = handle_request("test", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"].as_bool(), Some(true));
        assert_eq!(
            v["message"].as_str().unwrap(),
            "helper daemon connectivity works"
        );
    }

    #[test]
    fn test_validate_path_absolute() {
        let p = validate_path("/tmp", None).unwrap();
        assert_eq!(p, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_validate_path_nonexistent_rejected() {
        let err = validate_path("/nonexistent_path_xyz", None).unwrap_err();
        assert!(err.contains("path does not resolve"));
    }

    #[test]
    fn test_validate_path_outside_projects_root() {
        let root = Path::new("/tmp");
        let err = validate_path("/etc", Some(root)).unwrap_err();
        assert!(err.contains("outside allowed projects root"));
    }

    struct EnvGuard(&'static str, Option<std::ffi::OsString>);
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    // resolve_path tests share HOME env — must run serially
    #[test]
    fn test_resolve_path_variants() {
        let _guard = EnvGuard("HOME", std::env::var_os("HOME"));
        std::env::set_var("HOME", "/home/testuser");
        assert_eq!(resolve_path("~"), PathBuf::from("/home/testuser"));
        assert_eq!(resolve_path("~/foo"), PathBuf::from("/home/testuser/foo"));
        assert_eq!(resolve_path("/tmp/bar"), PathBuf::from("/tmp/bar"));
        let expected = std::env::current_dir().unwrap().join("relative/path");
        assert_eq!(resolve_path("relative/path"), expected);
    }
}
