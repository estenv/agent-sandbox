use std::os::unix::process::CommandExt;
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
    let mut command = Command::new(prog);
    command
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command.process_group(0);

    let child = command
        .spawn()
        .map_err(|e| format!("failed to spawn {label}: {e}"))?;

    let pgid = child.id();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => result.map_err(|e| format!("{label} wait failed: {e}")),
        Err(_) => {
            // Kill the entire process group (negative PID), not just the leader
            let _ = Command::new("kill")
                .arg("--")
                .arg(format!("-{}", pgid))
                .output();
            Err(format!("{label} timed out after {}s", timeout.as_secs()))
        }
    }
}
