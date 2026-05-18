/// Janet oracle — run Janet code and capture output.
///
/// The oracle executes Janet source code as a subprocess and
/// returns its stdout.  This is the verification step: if the
/// Janet program produces the same output as the Bash script,
/// the IR transformation is semantically correct.
use std::process::Command;

/// Run Janet code and return (exit_code, stdout, stderr).
pub fn run_janet(source: &str) -> Result<(i32, String, String), String> {
    let output = Command::new("janet")
        .arg("-e")
        .arg(source)
        .output()
        .map_err(|e| format!("failed to run janet: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((code, stdout, stderr))
}

/// Run Janet code and assert it succeeds, returning stdout.
pub fn run_janet_ok(source: &str) -> Result<String, String> {
    let (code, stdout, stderr) = run_janet(source)?;
    if code != 0 {
        Err(format!(
            "janet exited with code {}\nstderr: {}\nsource:\n{}",
            code, stderr, source
        ))
    } else {
        Ok(stdout)
    }
}

/// Run a Bash script and return (exit_code, stdout).
pub fn run_bash(source: &str) -> Result<(i32, String), String> {
    let output = Command::new("bash")
        .arg("-c")
        .arg(source)
        .output()
        .map_err(|e| format!("failed to run bash: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok((code, stdout))
}
