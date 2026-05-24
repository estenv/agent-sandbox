use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn run_output(
    prog: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    label: &str,
) -> Result<Output, String> {
    let child = Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn {label}: {e}"))?;

    let pid = child.id();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => result.map_err(|e| format!("{label} wait failed: {e}")),
        Err(_) => {
            let _ = Command::new("kill").arg(pid.to_string()).output();
            Err(format!("{label} timed out after {}s", timeout.as_secs()))
        }
    }
}
