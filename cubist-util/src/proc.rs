use std::process::Stdio;

use parse_display::Display;
use tokio::process::Command;

/// Signals that the `kill` function can send.
///
/// Each enum variant is named after the corresponding UNIX signal
/// (e.g., "Kill" corresponds to "SIGKILL"). These names are passed
/// (in upper case) as the argument to the kill command's `-s` flag.
#[derive(Display)]
#[display(style = "UPPERCASE")]
pub enum Signal {
    Int,
    Term,
}

pub const SIGINT: Signal = Signal::Int;
pub const SIGTERM: Signal = Signal::Term;

/// Kills a process `pid` by sending signal `sig`.
///
/// Works on *nix platofmrs only.
///
/// # Arguments
/// * `pid` - id of the process to kill
/// * `sig` - signal to send to the process
pub async fn kill(pid: u32, sig: Signal) -> tokio::io::Result<()> {
    // TODO: is there a direct way to kill an external process in rust?
    Command::new("kill")
        .args(["-s", sig.to_string().as_str(), pid.to_string().as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    Ok(())
}

/// Kills a process `pid` by sending signal `sig`.
///
/// Works on *nix platofmrs only.
///
/// # Arguments
/// * `pid` - id of the process to kill
/// * `sig` - signal to send to the process
pub fn kill_sync(pid: u32, sig: Signal) -> tokio::io::Result<()> {
    // TODO: is there a direct way to kill an external process in rust?
    std::process::Command::new("kill")
        .args(["-s", sig.to_string().as_str(), pid.to_string().as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_kill() {
        let mut cmd = tokio::process::Command::new("sleep")
            .arg("10000")
            .spawn()
            .unwrap();
        kill(cmd.id().unwrap(), SIGTERM).await.unwrap();
        timeout(Duration::from_secs(10), cmd.wait())
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_kill_sync() {
        let mut cmd = std::process::Command::new("sleep")
            .arg("10000")
            .spawn()
            .unwrap();
        kill_sync(cmd.id(), SIGTERM).unwrap();
        timeout(Duration::from_secs(10), async move { cmd.wait() })
            .await
            .unwrap()
            .unwrap();
    }
}
