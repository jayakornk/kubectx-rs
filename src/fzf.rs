// fzf integration for interactive context/namespace selection.
//
// If fzf is available and stdout is a terminal, kubectx uses it to present
// an interactive fuzzy-search menu.
//
// fzf is opened immediately and items are streamed to its stdin from a
// background thread, so the user can start typing/searching while data
// is still loading (e.g. `kubectl get namespaces` over the network).

#![allow(dead_code)]

use std::io::Write;
use std::process::{Command, Stdio};

use colored::Colorize;

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

/// Open fzf immediately, then run `loader` in a background thread to fetch
/// items. Items are written to fzf's stdin as soon as they're available so
/// the user can start searching while data is still loading.
///
/// `header` is shown as a static header line in fzf (e.g. "⏳ Loading namespaces…").
/// `current` highlights the currently-active item in cyan/bold via ANSI
/// color codes. fzf's `--ansi` flag renders them for display but strips them
/// from its stdout, so the returned item needs no prefix cleanup.
/// `loader` runs on a background thread and returns the full list of items.
///
/// Returns the selected item, or None if the user cancelled or fzf failed.
pub fn fuzzy_select_streaming(
    current: Option<&str>,
    loader: impl FnOnce() -> Vec<String> + Send + 'static,
) -> Option<String> {
    let fzf_path = which_fzf()?;

    // Open fzf immediately — its UI is interactive from the start, reading
    // from stdin in the background. Items appear as they arrive.
    let mut child = Command::new(&fzf_path)
        .arg("--ansi")
        .arg("--no-preview")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // Take ownership of stdin so the background thread can write to it.
    let stdin = child.stdin.take()?;
    let current_owned = current.map(|s| s.to_string());

    // Spawn a background thread to fetch data and stream items to fzf.
    // fzf is already open and interactive — items will appear as they
    // are written, and stdin EOF (via drop) signals the list is complete.
    let handle = std::thread::spawn(move || {
        let items = loader();
        let mut stdin = stdin;
        for item in &items {
            if Some(item.as_str()) == current_owned.as_deref() {
                // Highlight the current item via ANSI color; fzf's --ansi
                // flag renders it in display but returns plain text.
                let line = format!("{}\n", item.cyan().bold());
                let _ = stdin.write_all(line.as_bytes());
            } else {
                let _ = stdin.write_all(format!("{}\n", item).as_bytes());
            }
        }
        drop(stdin); // EOF — tells fzf the list is complete
    });

    let output = child.wait_with_output().ok()?;
    handle.join().ok()?;

    if !output.status.success() {
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
    Some(trimmed.to_string())
}
