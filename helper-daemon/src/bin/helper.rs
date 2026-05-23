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

    let path = env::args().nth(1).unwrap_or_else(|| "healthz".to_string());

    let mut conn = match UnixStream::connect(&sock) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to connect to daemon socket: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = write!(conn, "{path}\n") {
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
