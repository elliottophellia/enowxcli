use super::{string_arg, truncate_output, Tool, ToolCtx, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

pub(super) struct BashTool;

/// `command` run by the platform's shell: `/bin/sh` on Unix (a login shell
/// when `login`); on Windows the `sh` Git for Windows puts on PATH, or
/// PowerShell when there is none.
pub(crate) fn shell_command(command: &str, login: bool) -> Command {
    #[cfg(unix)]
    {
        let mut shell = Command::new("/bin/sh");
        shell.arg(if login { "-lc" } else { "-c" }).arg(command);
        shell
    }
    #[cfg(not(unix))]
    {
        let _ = login;
        if which::which("sh").is_ok() {
            let mut shell = Command::new("sh");
            shell.arg("-c").arg(command);
            shell
        } else {
            let mut shell = Command::new("powershell");
            shell.args(["-NoProfile", "-Command", command]);
            shell
        }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run one shell command in the workspace to build, test, install, or run something. \
         stdout and stderr are returned together. Not for reading or listing files: \
         use read, glob and grep for that."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let command = string_arg(&args, "command")?;
        let mut spawner = shell_command(command, true);
        spawner
            .current_dir(&ctx.workspace)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // Its own process group, so a stop or timeout reaches the whole pipeline
        // rather than leaving `sh`'s children running.
        #[cfg(unix)]
        spawner.process_group(0);
        let mut child = spawner.spawn().context("starting shell")?;
        let pid = child.id();

        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let wait =
            async { tokio::try_join!(child.wait(), capture_pipe(stdout), capture_pipe(stderr)) };
        let (status, stdout, stderr) = tokio::select! {
            result = tokio::time::timeout(ctx.shell_timeout, wait) => match result {
                Ok(output) => output.context("collecting command output")?,
                Err(_) => {
                    stop_tree(&mut child, pid).await;
                    anyhow::bail!("command timed out after {} seconds", ctx.shell_timeout.as_secs());
                }
            },
            _ = ctx.cancel.cancelled() => {
                stop_tree(&mut child, pid).await;
                anyhow::bail!("command cancelled");
            }
        };

        let mut text = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = String::from_utf8_lossy(&stderr);
        if !stderr.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&stderr);
        }
        let exit = status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string());
        truncate_output(&mut text, 80_000);
        let content = format!("exit {exit}\n{}", text.trim_end());
        Ok(if status.success() {
            ToolOutput::ok(content)
        } else {
            ToolOutput::error(content)
        })
    }
}

/// End a command and everything it started, then reap it, without ever
/// waiting long: a stop or a timeout must end the tool call even when
/// something in the tree will not die.
///
/// Unix: the shell leads its own process group, so the whole group gets
/// SIGKILL. Windows has no groups to signal, and killing `sh` alone left its
/// children (a dev server, a watcher, a prompt waiting for input) running
/// with the output pipes open, and the wait for the shell never returned: the
/// agent sat on the call for good and Stop did nothing. `taskkill /T /F`
/// ends the tree there.
async fn stop_tree(child: &mut tokio::process::Child, pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    #[cfg(windows)]
    if let Some(pid) = pid {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status(),
        )
        .await;
    }
    #[cfg(not(any(unix, windows)))]
    let _ = pid;
    let _ = child.start_kill();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
}

async fn capture_pipe(mut pipe: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = pipe.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let retained = count.min(80_000usize.saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < count;
    }
    if truncated {
        output.extend_from_slice(b"\n[output truncated]");
    }
    Ok(output)
}
