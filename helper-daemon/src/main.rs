use clap::Parser;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox-helper-daemon")]
#[command(about = "Host helper daemon for agent-sandbox — listens on a Unix socket")]
#[command(version)]
struct Cli {
    /// Path to the Unix domain socket.
    #[arg(long, default_value = "~/.agent-sandbox/daemon.sock")]
    socket_path: String,
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

    let _ = fs::remove_file(&socket_path);

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    eprintln!(
        "agent-sandbox-helper-daemon listening on {}",
        socket_path.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(err) = handle_connection(stream) {
                    eprintln!("request failed: {err}");
                }
            }
            Err(err) => eprintln!("accept failed: {err}"),
        }
    }

    Ok(())
}

fn handle_request(first_line: &str) -> (&'static str, &'static str) {
    match first_line {
        line if line.starts_with("GET /healthz ") => (
            "HTTP/1.1 200 OK",
            r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#,
        ),
        line if line.starts_with("GET /v1/test ") => (
            "HTTP/1.1 200 OK",
            r#"{"ok":true,"message":"helper daemon connectivity works"}"#,
        ),
        _ => (
            "HTTP/1.1 404 Not Found",
            r#"{"ok":false,"error":"not found"}"#,
        ),
    }
}

fn handle_connection(mut stream: UnixStream) -> std::io::Result<()> {
    let mut buffer = [0_u8; 4096];
    let n = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or_default();
    let (status, body) = handle_request(first_line);

    write!(
        stream,
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
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
        let (status, body) = handle_request("GET /healthz HTTP/1.1");
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#);
    }

    #[test]
    fn test_handle_v1_test() {
        let (status, body) = handle_request("GET /v1/test HTTP/1.1");
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, r#"{"ok":true,"message":"helper daemon connectivity works"}"#);
    }

    #[test]
    fn test_handle_not_found() {
        let (status, body) = handle_request("GET /nonexistent HTTP/1.1");
        assert_eq!(status, "HTTP/1.1 404 Not Found");
        assert_eq!(body, r#"{"ok":false,"error":"not found"}"#);
    }

    #[test]
    fn test_handle_empty_line_returns_404() {
        let (status, _) = handle_request("");
        assert_eq!(status, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn test_handle_healthz_needs_trailing_space() {
        let (status, _) = handle_request("GET /healthz");
        assert_eq!(status, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn test_resolve_path_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(resolve_path("~/foo"), PathBuf::from(home).join("foo"));
    }

    #[test]
    fn test_resolve_path_tilde_only() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(resolve_path("~"), PathBuf::from(home));
    }

    #[test]
    fn test_resolve_path_absolute() {
        assert_eq!(resolve_path("/tmp/bar"), PathBuf::from("/tmp/bar"));
    }

    #[test]
    fn test_resolve_path_relative() {
        assert_eq!(resolve_path("relative/path"), PathBuf::from("relative/path"));
    }
}
