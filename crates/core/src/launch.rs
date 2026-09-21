use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::AgentProfile;

pub struct SessionCapture {
    pub success: bool,
    pub input: String,
    pub output: String,
}

pub fn launch(profile: &AgentProfile, worktree: &Path, context: &str) -> Result<SessionCapture> {
    launch_inner(profile, worktree, context, true)
}

fn launch_inner(
    profile: &AgentProfile,
    worktree: &Path,
    context: &str,
    forward_stdin: bool,
) -> Result<SessionCapture> {
    let (cols, rows) = size().unwrap_or((80, 24));
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open a pseudo-terminal")?;

    let mut command = CommandBuilder::new(&profile.command);
    for arg in &profile.args {
        command.arg(arg);
    }
    command.arg(context);
    command.cwd(worktree);

    let mut child = pair
        .slave
        .spawn_command(command)
        .with_context(|| format!("failed to start agent command '{}'", profile.command))?;
    drop(pair.slave);

    let captured_input = Arc::new(Mutex::new(Vec::new()));
    let input_buffer = Arc::clone(&captured_input);
    let pty_writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    if forward_stdin {
        let input_writer = Arc::clone(&pty_writer);
        thread::spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 1024];
            while let Ok(count) = stdin.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                input_buffer
                    .lock()
                    .unwrap()
                    .extend_from_slice(&buffer[..count]);
                let Ok(mut writer) = input_writer.lock() else {
                    break;
                };
                if writer.write_all(&buffer[..count]).is_err() || writer.flush().is_err() {
                    break;
                }
            }
        });
    }

    let captured_output = Arc::new(Mutex::new(Vec::new()));
    let output_buffer = Arc::clone(&captured_output);
    let mut response_writer = (!forward_stdin).then(|| Arc::clone(&pty_writer));
    let mut pty_reader = pair.master.try_clone_reader()?;
    let output_thread = thread::spawn(move || -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = match pty_reader.read(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => break,
                Err(error) => return Err(error),
            };
            if count == 0 {
                break;
            }
            if buffer[..count].windows(4).any(|bytes| bytes == b"\x1b[6n")
                && let Some(writer) = response_writer.take()
            {
                let mut writer = writer.lock().unwrap();
                writer.write_all(b"\x1b[1;1R")?;
                writer.flush()?;
            }
            output_buffer
                .lock()
                .unwrap()
                .extend_from_slice(&buffer[..count]);
            stdout.write_all(&buffer[..count])?;
            stdout.flush()?;
        }
        Ok(())
    });

    let raw_mode = if forward_stdin {
        RawMode::enable()
    } else {
        RawMode(false)
    };
    let status = child.wait().context("failed while waiting for agent")?;
    drop(raw_mode);
    drop(pty_writer);
    output_thread
        .join()
        .map_err(|_| anyhow::anyhow!("agent output thread panicked"))??;

    let input = sanitize(&captured_input.lock().unwrap());
    let output = sanitize(&captured_output.lock().unwrap());
    Ok(SessionCapture {
        success: status.success(),
        input,
        output,
    })
}

fn sanitize(bytes: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(bytes);
    String::from_utf8_lossy(&stripped)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect::<String>()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

struct RawMode(bool);

impl RawMode {
    fn enable() -> Self {
        Self(enable_raw_mode().is_ok())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = disable_raw_mode();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_and_control_characters() {
        assert_eq!(sanitize(b"\x1b[31mhello\x1b[0m\r\nworld\0"), "hello\nworld");
    }

    #[test]
    fn captures_process_output_through_pty() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let profile = AgentProfile {
            command: "cmd.exe".into(),
            args: vec!["/C".into(), "echo".into()],
        };
        #[cfg(not(windows))]
        let profile = AgentProfile {
            command: "echo".into(),
            args: Vec::new(),
        };

        let capture = launch_inner(&profile, directory.path(), "akh-context", false).unwrap();
        assert!(capture.success);
        assert!(capture.output.contains("akh-context"));
        assert!(capture.input.is_empty());
    }
}
