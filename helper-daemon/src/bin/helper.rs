use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

fn main() -> ExitCode {
    let sock = match env::var("HELPER_DAEMON_SOCK") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("error: HELPER_DAEMON_SOCK is not set");
            return ExitCode::from(1);
        }
    };

    let mut path: String = env::args().skip(1).collect::<Vec<_>>().join(" ");
    if path.is_empty() {
        path = "healthz".to_string();
    }

    let mut conn = match UnixStream::connect(&sock) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to connect to daemon socket: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = writeln!(conn, "{path}") {
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
