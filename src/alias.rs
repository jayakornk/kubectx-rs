// Context alias management.
#![allow(dead_code)]
//
// Aliases are short user-defined names that map to full context names.
// Stored in ~/.kube/kubectx/aliases as simple `alias=context` lines.
//
// Usage:
//   kubectx @prod              → switch to context aliased as "prod"
//   kubectx @prod=gke_long_name → create/set alias "prod" → "gke_long_name"
//   kubectx --aliases          → list all aliases
//   kubectx -d @prod           → delete alias "prod"

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

/// Get the path to the aliases file.
/// Reuses the ~/.kube/kubectx/ directory managed by the state module
/// (which handles migration from the old file-at-~/.kube/kubectx format).
fn aliases_file() -> Option<PathBuf> {
    let prev = crate::state::prev_context_file()?;
    let dir = prev.parent()?;
    Some(dir.join("aliases"))
}

/// Load all aliases as (alias, context) pairs.
pub fn load_aliases() -> Vec<(String, String)> {
    let path = match aliases_file() {
        Some(p) => p,
        None => return Vec::new(),
    };
    match fs::File::open(&path) {
        Ok(f) => {
            let reader = BufReader::new(f);
            let mut aliases = Vec::new();
            for line in reader.lines() {
                if let Ok(l) = line {
                    let l = l.trim();
                    if l.is_empty() || l.starts_with('#') {
                        continue;
                    }
                    if let Some(eq) = l.find('=') {
                        let alias = l[..eq].trim().to_string();
                        let ctx = l[eq + 1..].trim().to_string();
                        if !alias.is_empty() && !ctx.is_empty() {
                            aliases.push((alias, ctx));
                        }
                    }
                }
            }
            aliases
        }
        Err(_) => Vec::new(),
    }
}

/// Look up an alias and return the full context name.
pub fn resolve_alias(alias: &str) -> Option<String> {
    load_aliases()
        .into_iter()
        .find(|(a, _)| a == alias)
        .map(|(_, ctx)| ctx)
}

/// Save or update an alias.
pub fn set_alias(alias: &str, context: &str) -> io::Result<()> {
    let mut aliases = load_aliases();
    // Remove existing alias if present
    aliases.retain(|(a, _)| a != alias);
    aliases.push((alias.to_string(), context.to_string()));
    write_aliases(&aliases)
}

/// Delete an alias. Returns true if it existed.
pub fn delete_alias(alias: &str) -> io::Result<bool> {
    let mut aliases = load_aliases();
    let before = aliases.len();
    aliases.retain(|(a, _)| a != alias);
    let existed = aliases.len() < before;
    if existed {
        write_aliases(&aliases)?;
    }
    Ok(existed)
}

/// Write all aliases to the file.
fn write_aliases(aliases: &[(String, String)]) -> io::Result<()> {
    let path = aliases_file().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "cannot determine home directory")
    })?;
    let mut file = fs::File::create(&path)?;
    for (alias, ctx) in aliases {
        writeln!(file, "{}={}", alias, ctx)?;
    }
    Ok(())
}

/// Check if a string looks like an alias reference (starts with @).
pub fn is_alias_ref(s: &str) -> bool {
    s.starts_with('@') && s.len() > 1
}

/// Strip the @ prefix from an alias reference.
pub fn strip_at(s: &str) -> &str {
    s.strip_prefix('@').unwrap_or(s)
}
