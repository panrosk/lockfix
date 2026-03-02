use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct CommandError {
    pub command: String,
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Command '{}' failed in '{}': {}",
            self.command, self.path, self.message
        )
    }
}

impl std::error::Error for CommandError {}

pub fn run_command(cmd: &str, cwd: &Path) -> Result<(), CommandError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(CommandError {
            command: cmd.to_string(),
            path: cwd.display().to_string(),
            message: "empty command".into(),
        });
    }

    let mut command = Command::new(parts[0]);
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    command.current_dir(cwd);

    let output = command.output().map_err(|e| CommandError {
        command: cmd.to_string(),
        path: cwd.display().to_string(),
        message: e.to_string(),
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError {
            command: cmd.to_string(),
            path: cwd.display().to_string(),
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}
