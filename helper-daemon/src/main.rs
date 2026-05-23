use clap::Parser;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox-helper-daemon")]
#[command(about = "No-op host helper daemon skeleton for agent-sandbox")]
#[command(version)]
struct Cli {
    /// Address to bind.
    #[arg(long, default_value = "127.0.0.1:47688")]
    bind: String,
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
    let listener = TcpListener::bind(&cli.bind)?;
    eprintln!(
        "agent-sandbox-helper-daemon listening on http://{}",
        cli.bind
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

fn handle_connection(mut stream: TcpStream) -> std::io::Result<()> {
    let mut buffer = [0_u8; 4096];
    let n = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or_default();

    let (status, body) = match first_line {
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
    };

    write!(
        stream,
        "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
