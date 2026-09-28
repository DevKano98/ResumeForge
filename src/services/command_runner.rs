use std::process::Output;
use tokio::process::Command;

pub async fn run_command(program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
    match Command::new(program).args(args).output().await {
        Ok(out) => Ok(out),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && cfg!(windows) => {
            // Fallback check in known user bin dirs on Windows
            if let Ok(user_profile) = std::env::var("USERPROFILE") {
                let fallback_dirs = [
                    format!("{}\\AppData\\Local\\agy\\bin", user_profile),
                    format!("{}\\w64devkit\\bin", user_profile),
                    format!("{}\\AppData\\Local\\Microsoft\\WinGet\\Links", user_profile),
                    "C:\\Program Files\\GitHub CLI".to_string(),
                ];
                for dir in &fallback_dirs {
                    let exe_path = format!("{}\\{}.exe", dir, program);
                    if std::path::Path::new(&exe_path).exists() {
                        if let Ok(out) = Command::new(&exe_path).args(args).output().await {
                            return Ok(out);
                        }
                    }
                }
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

pub async fn check_installed(program: &str, version_arg: &str) -> Option<String> {
    match run_command(program, &[version_arg]).await {
        Ok(out) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        _ => None,
    }
}
