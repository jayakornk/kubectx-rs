// State file management for tracking previous context/namespace.
//
// Previous context is stored in ~/.kube/kubectx/prev_context
// Previous namespace is stored in ~/.kube/kubens/prev_namespace

#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::PathBuf;

/// Get the path to the previous context state file.
pub fn prev_context_file() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".kube").join("kubectx");
    fs::create_dir_all(&dir).ok()?;
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