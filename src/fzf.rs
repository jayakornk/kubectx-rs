// fzf integration for interactive context/namespace selection.
//
// If fzf is available and stdout is a terminal, kubectx uses it to present
// an interactive fuzzy-search menu.

#![allow(dead_code)]

use std::io::Write;
use std::process::{Command, Stdio};

/// Check if fzf is available in PATH and not disabled via env.
pub fn fzf_available() -> bool {
    if std::env::var("KUBECTX_IGNORE_FZF").as_deref() == Ok("1") {
        return false;
    }
    which_fzf().is_some()
}

/// Find the path to fzf, if it exists.
fn which_fzf() -> Option<String> {
    let result = Command::new("which")
        .arg("fzf")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    let path = String::from_utf8(result.stdout).ok()?;
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Present items via fzf and return the selected one.
/// Returns None if user cancelled (Esc, Ctrl-C, etc.) or on error.
pub fn fuzzy_select(items: &[String], current: Option<&str>) -> Option<String> {
    let fzf_path = which_fzf()?;

    let mut child = Command::new(&fzf_path)
        .arg("--ansi")
        .arg("--no-preview")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // Write items to fzf stdin, highlighting the current one
    if let Some(mut stdin) = child.stdin.take() {
        for item in items {
            if Some(item.as_str()) == current {
                // Mark current with an asterisk and color
                let line = format!("{} {}\n", "*".yellow(), item.cyan());
                let _ = stdin.write_all(line.as_bytes());
            } else {
                let _ = stdin.write_all(format!("  {}\n", item).as_bytes());
            }
        }
        // Drop stdin to signal EOF
        drop(stdin);
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        // Show fzf errors if any
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprintln!("{} fzf: {}", "warning:".yellow(), stderr.trim());
        }
        return None;
    }

    let selected = String::from_utf8(output.stdout).ok()?;
    let trimmed = selected.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip the leading "  " or "* " prefix
    Some(trimmed.trim_start().to_string())
}

use colored::Colorize;