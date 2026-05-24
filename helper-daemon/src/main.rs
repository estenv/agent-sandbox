mod ado;
mod deps;
mod git;

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    eprintln!(
        "agent-sandbox-helper-daemon listening on {}",
        socket_path.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(ref err) = handle_connection(stream, projects_root.as_deref()) {
                    if err.kind() != std::io::ErrorKind::BrokenPipe {
                        eprintln!("request failed: {err}");
                    }
                }
            }
            Err(err) => eprintln!("accept failed: {err}"),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: UnixStream, projects_root: Option<&Path>) -> std::io::Result<()> {
    let mut buffer = [0_u8; 4096];
    let n = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..n]);
    let line = request.lines().next().unwrap_or_default().trim().to_owned();
    let body = handle_request(&line, projects_root);
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

pub(crate) fn ok_response(data: serde_json::Value) -> String {
    let mut resp = serde_json::json!({"ok": true});
    if let serde_json::Value::Object(ref mut obj) = resp {
        if let serde_json::Value::Object(extra) = data {
            obj.extend(extra);
        }
    }
    resp.to_string()
}

pub(crate) fn err_response(error: impl Into<String>) -> String {
    serde_json::json!({"ok": false, "error": error.into()}).to_string()
}

pub(crate) fn validate_path<'a>(
    path_str: &'a str,
    projects_root: Option<&Path>,
) -> Result<&'a Path, String> {
    let cwd = Path::new(path_str);
    if !cwd.is_absolute() {
        return Err("path must be absolute".into());
    }
    if let Some(root) = projects_root {
        if !cwd.starts_with(root) {
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
            Ok(cwd) => git::git_pull(cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::GitPush { path } => match validate_path(&path, projects_root) {
            Ok(cwd) => git::git_push(cwd),
            Err(e) => err_response(e),
        },
        protocol::DaemonCommand::PrCreate {
            path,
            title,
            source,
            target,
            description,
            work_item,
        } => {
            let params = ado::PrParams {
                path,
                title,
                source,
                target,
                description,
                work_item,
            };
            ado::pr_create(&params, projects_root)
        }
        protocol::DaemonCommand::DepInstall { path } => {
            let dir = Path::new(&path);
            deps::dep_install(dir, projects_root)
        }
        protocol::DaemonCommand::WiList { path } => ado::wi_list(&path, projects_root),
        protocol::DaemonCommand::WiCreate {
            path,
            title,
            parent,
            description,
            r#type,
        } => ado::wi_create(
            &path,
            &title,
            parent,
            description.as_deref(),
            r#type.as_deref(),
            projects_root,
        ),
    }
}

fn resolve_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            let home = PathBuf::from(home);
            if rest.is_empty() || rest == "/" {
                return home;
            }
            let rest = rest.trim_start_matches('/');
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_healthz() {
        let body = handle_request("healthz", None);
        assert_eq!(
            body,
            r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#
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
        assert_eq!(
            validate_path("/valid/path", None).unwrap(),
            Path::new("/valid/path")
        );
    }

    #[test]
    fn test_validate_path_relative_rejected() {
        let err = validate_path("relative/path", None).unwrap_err();
        assert_eq!(err, "path must be absolute");
    }

    #[test]
    fn test_validate_path_outside_projects_root() {
        let root = Path::new("/allowed");
        let err = validate_path("/forbidden", Some(root)).unwrap_err();
        assert!(err.contains("outside allowed projects root"));
    }

    #[test]
    fn test_resolve_path_tilde() {
        std::env::set_var("HOME", "/home/testuser");
        assert_eq!(resolve_path("~/foo"), PathBuf::from("/home/testuser/foo"));
    }

    #[test]
    fn test_resolve_path_tilde_only() {
        std::env::set_var("HOME", "/home/testuser");
        assert_eq!(resolve_path("~"), PathBuf::from("/home/testuser"));
    }

    #[test]
    fn test_resolve_path_absolute() {
        assert_eq!(resolve_path("/tmp/bar"), PathBuf::from("/tmp/bar"));
    }

    #[test]
    fn test_resolve_path_relative() {
        assert_eq!(
            resolve_path("relative/path"),
            PathBuf::from("relative/path")
        );
    }
}
