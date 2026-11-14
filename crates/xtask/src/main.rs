mod comments;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn repository_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err("not inside a git repository".into());
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn tracked_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err("git ls-files failed".into());
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| root.join(String::from_utf8_lossy(name).as_ref()))
        .collect())
}

fn check_comments() -> Result<Vec<String>, String> {
    let root = repository_root()?;
    let mut problems = Vec::new();
    for path in tracked_files(&root)? {
        let Some(checker) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(comments::checker_for)
        else {
            continue;
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        problems.extend(
            checker(&text)
                .into_iter()
                .map(|line| format!("{relative}:{line}")),
        );
    }
    Ok(problems)
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    if task != "check-comments" {
        eprintln!("usage: cargo xtask check-comments");
        return ExitCode::FAILURE;
    }
    match check_comments() {
        Ok(problems) if problems.is_empty() => ExitCode::SUCCESS,
        Ok(problems) => {
            println!("Comments are not allowed in code:");
            for problem in problems {
                println!("{problem}");
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
