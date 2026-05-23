use std::env;
use std::ffi::OsString;
use std::io;
use std::process::Command;

pub fn prepare(agent: &str) -> Result<(), Box<dyn std::error::Error>> {
    match agent {
        "opencode" => npm_install_global("opencode-ai")?,
        "pi" | "pi-agent" => npm_install_global("@mariozechner/pi-coding-agent")?,
        "claude" => npm_install_global("@anthropic-ai/claude-code")?,
        other => return Err(format!("no preparation recipe for `{other}`").into()),
    }
    Ok(())
}

pub fn known_for_command(command: &str) -> Option<&'static str> {
    match command {
        "opencode" => Some("opencode"),
        "pi" | "pi-agent" => Some("pi"),
        "claude" => Some("claude"),
        _ => None,
    }
}

fn npm_install_global(package: &str) -> io::Result<()> {
    let npm = env::var_os("AGENT_SANDBOX_NPM").unwrap_or_else(|| OsString::from("npm"));
    let status = Command::new(npm)
        .arg("install")
        .arg("-g")
        .arg(package)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("npm install -g {package} failed")))
    }
}
