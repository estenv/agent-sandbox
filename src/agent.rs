use std::env;
use std::ffi::OsString;
use std::io;
use std::process::Command;

pub fn prepare(agent: &str) -> Result<(), Box<dyn std::error::Error>> {
    match agent {
        "opencode" => npm_install_global("opencode-ai")?,
        "pi" | "pi-agent" => npm_install_global("@mariozechner/pi-coding-agent")?,
        other => return Err(format!("no preparation recipe for `{other}`").into()),
    }
    Ok(())
}

pub fn known_for_command(command: &str) -> Option<&'static str> {
    match command {
        "opencode" => Some("opencode"),
        "pi" | "pi-agent" => Some("pi"),
        _ => None,
    }
}

pub fn env_vars(agent: &str) -> &[(&'static str, &'static str)] {
    match agent {
        "opencode" => &[("OPENCODE_DISABLE_AUTOUPDATE", "true")],
        "pi" => &[("PI_OFFLINE", "true")],
        _ => &[],
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
