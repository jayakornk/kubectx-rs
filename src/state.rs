// State file management for tracking previous context/namespace.
//
// Previous context is stored in ~/.kube/kubectx/prev_context
// Previous namespace is stored in ~/.kube/kubens/prev_namespace
//
// Handles migration from the old format where ~/.kube/kubectx was a file
// (used by older versions of kubectx) to the directory-based format.

#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Ensure a directory exists, migrating away if a file exists at that path.
fn ensure_dir(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        // A file exists where we need a directory — migrate it.
        // Read the old file content, remove it, create the directory,
        // then write the content to the proper file inside.
        let old_content = fs::read_to_string(path).unwrap_or_default();
        fs::remove_file(path)?;
        fs::create_dir_all(path)?;
        // Write the old content to prev_context inside the new directory
        if !old_content.trim().is_empty() {
            fs::write(path.join("prev_context"), old_content.trim())?;
        }
        Ok(())
    } else {
        fs::create_dir_all(path)
    }
}

/// Get the path to the previous context state file.
pub fn prev_context_file() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".kube").join("kubectx");
    ensure_dir(&dir).ok()?;
    Some(dir.join("prev_context"))
}

/// Get the path to the previous namespace state file.
pub fn prev_namespace_file() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".kube").join("kubens");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("prev_namespace"))
}

/// Read the last context/namespace from the state file.
pub fn read_state(path: &PathBuf) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content.trim().to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Write the context/namespace to the state file.
pub fn write_state(path: &PathBuf, value: &str) -> io::Result<()> {
    fs::write(path, value)
}