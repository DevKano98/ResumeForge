use std::process::Output;
use tokio::process::Command;

pub async fn run_command(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    // 1. Check tools/bin next to app first
    let tools_bin = std::path::PathBuf::from("tools").join("bin");
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let candidate = tools_bin.join(format!("{}{}", program, ext));
    if candidate.exists() {
        if let Ok(out) = Command::new(&candidate).args(args).output().await {
            return Ok(out);
        }
    }

    // 2. Lookup on PATH
    Command::new(program).args(args).output().await
}

pub async fn check_installed(program: &str, version_arg: &str) -> Option<String> {
    match run_command(program, &[version_arg]).await {
        Ok(out) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        _ => None,
    }
}
