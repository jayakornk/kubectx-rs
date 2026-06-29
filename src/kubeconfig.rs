// Kubeconfig loading, parsing, and modification.
//
// Supports the KUBECONFIG environment variable (colon-separated on Unix,
// semicolon on Windows) and falls back to ~/.kube/config.
//
// Multiple files are merged for reading (contexts collected from all files).
// For writing, the file that contains the relevant entry is modified,
// or the first file if none matches.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml::Value;

const KUBECONFIG_ENV: &str = "KUBECONFIG";

/// A single kubeconfig file entry with its parsed YAML root.
#[allow(dead_code)]
pub struct KubeconfigFile {
    pub path: PathBuf,
    pub document: Value,
}

impl KubeconfigFile {
    /// Load and parse a single kubeconfig file.
    pub fn load(path: &Path) -> Result<Self, KubeconfigError> {
        let contents = fs::read_to_string(path).map_err(|e| {
            KubeconfigError::Io(format!("failed to read {}: {}", path.display(), e))
        })?;
        let doc: Value = if contents.trim().is_empty() {
            Value::Mapping(serde_yaml::Mapping::new())
        } else {
            serde_yaml::from_str(&contents).map_err(|e| {
                KubeconfigError::Parse(format!("failed to parse {}: {}", path.display(), e))
            })?
        };
        Ok(Self {
            path: path.to_path_buf(),
            document: doc,
        })
    }

    /// Serialize and save back to disk.
    pub fn save(&self) -> Result<(), KubeconfigError> {
        let yaml = serde_yaml::to_string(&self.document).map_err(|e| {
            KubeconfigError::Parse(format!("failed to serialize {}: {}", self.path.display(), e))
        })?;
        fs::write(&self.path, yaml).map_err(|e| {
            KubeconfigError::Io(format!("failed to write {}: {}", self.path.display(), e))
        })
    }
}

/// Collection of loaded kubeconfig files.
#[allow(dead_code)]
pub struct Kubeconfig {
    files: Vec<KubeconfigFile>,
}

impl Kubeconfig {
    #![allow(dead_code)]
    /// Load from the default location(s): KUBECONFIG env var or ~/.kube/config.
    pub fn load_default() -> Result<Self, KubeconfigError> {
        let paths = resolve_kubeconfig_paths();
        let mut files = Vec::new();
        for p in &paths {
            if !p.exists() {
                continue;
            }
            match KubeconfigFile::load(p) {
                Ok(f) => files.push(f),
                Err(e) => {
                    // Skip files that don't exist; error on parse failures
                    if !matches!(e, KubeconfigError::Io(_)) {
                        return Err(e);
                    }
                }
            }
        }
        if files.is_empty() && paths.iter().any(|p| p.exists()) {
            // Files exist but all failed to load
            return Err(KubeconfigError::Io(
                "failed to load any kubeconfig files".to_string(),
            ));
        }
        Ok(Self { files })
    }

    /// Return all loaded file entries.
    pub fn files(&self) -> &[KubeconfigFile] {
        &self.files
    }

    /// Get the first file (primary write target).
    pub fn first_file_mut(&mut self) -> Option<&mut KubeconfigFile> {
        self.files.first_mut()
    }

    /// Get the current-context value. Returns the first non-empty one across files.
    pub fn get_current_context(&self) -> Option<String> {
        for f in &self.files {
            if let Some(ctx) = get_field_str(&f.document, "current-context") {
                if !ctx.is_empty() {
                    return Some(ctx);
                }
            }
        }
        None
    }

    /// Set the current-context field in the first file (or the file that has it).
    pub fn set_current_context(&mut self, ctx: &str) -> Result<(), KubeconfigError> {
        // Try to find the file that already has current-context set
        for f in &mut self.files {
            if get_field_str(&f.document, "current-context").is_some() {
                set_field_str(&mut f.document, "current-context", ctx);
                return Ok(());
            }
        }
        // Otherwise set it in the first file
        if let Some(f) = self.files.first_mut() {
            set_field_str(&mut f.document, "current-context", ctx);
            Ok(())
        } else {
            Err(KubeconfigError::Io(
                "no kubeconfig files loaded".to_string(),
            ))
        }
    }

    /// Unset the current-context field in all files.
    pub fn unset_current_context(&mut self) -> Result<(), KubeconfigError> {
        for f in &mut self.files {
            if let Value::Mapping(ref mut map) = f.document {
                map.remove(Value::String("current-context".into()));
            }
        }
        Ok(())
    }

    /// Collect all context names from all files.
    pub fn get_contexts(&self) -> Vec<String> {
        let mut contexts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for f in &self.files {
            for name in get_context_names(&f.document) {
                if seen.insert(name.clone()) {
                    contexts.push(name);
                }
            }
        }
        contexts
    }

    /// Check if a context exists in any file.
    pub fn context_exists(&self, name: &str) -> bool {
        self.get_contexts().iter().any(|c| c == name)
    }

    /// Delete a context by name. Returns (was_active, was_found).
    pub fn delete_context(&mut self, name: &str) -> (bool, bool) {
        let current = self.get_current_context();
        let mut found = false;
        for f in &mut self.files {
            if remove_context(&mut f.document, name) {
                found = true;
            }
        }
        let was_active = current.as_deref() == Some(name);
        (was_active, found)
    }

    /// Rename a context. Returns Ok(()) or an error.
    /// `source` can be "." to mean the current context.
    pub fn rename_context(&mut self, new_name: &str, source: &str) -> Result<(), KubeconfigError> {
        let source = if source == "." {
            self.get_current_context()
                .ok_or_else(|| KubeconfigError::Other("no current context set".into()))?
        } else {
            source.to_string()
        };

        if new_name == source {
            return Err(KubeconfigError::Other(
                "new name and old name are the same".into(),
            ));
        }
        if self.context_exists(new_name) {
            return Err(KubeconfigError::Other(format!(
                "context \"{}\" already exists",
                new_name
            )));
        }
        if !self.context_exists(&source) {
            return Err(KubeconfigError::Other(format!(
                "context \"{}\" not found",
                source
            )));
        }

        for f in &mut self.files {
            rename_context_entry(&mut f.document, &source, new_name);
        }
        // If the renamed context was the current context, update current-context
        if self.get_current_context().as_deref() == Some(source.as_str()) {
            self.set_current_context(new_name)?;
        }
        Ok(())
    }

    /// Get the namespace of the current context.
    pub fn get_current_namespace(&self) -> Option<String> {
        let ctx = self.get_current_context()?;
        for f in &self.files {
            if let Some(ns) = get_context_namespace(&f.document, &ctx) {
                return Some(ns);
            }
        }
        None
    }

    /// Set the namespace for the current context.
    pub fn set_current_namespace(&mut self, namespace: &str) -> Result<(), KubeconfigError> {
        let ctx = self
            .get_current_context()
            .ok_or_else(|| KubeconfigError::Other("no current context set".into()))?;
        for f in &mut self.files {
            if set_context_namespace(&mut f.document, &ctx, namespace) {
                return Ok(());
            }
        }
        Err(KubeconfigError::Other(format!(
            "context \"{}\" not found in any kubeconfig file",
            ctx
        )))
    }

    /// Unset the namespace for the current context.
    pub fn unset_current_namespace(&mut self) -> Result<(), KubeconfigError> {
        let ctx = self
            .get_current_context()
            .ok_or_else(|| KubeconfigError::Other("no current context set".into()))?;
        for f in &mut self.files {
            if unset_context_namespace(&mut f.document, &ctx) {
                return Ok(());
            }
        }
        // If no namespace was set, that's ok - nothing to unset
        Ok(())
    }

    /// Collect all namespaces from the current context across all files.
    pub fn get_namespaces(&self) -> Vec<String> {
        let ctx = match self.get_current_context() {
            Some(c) => c,
            None => return Vec::new(),
        };
        let mut namespaces = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for f in &self.files {
            for ns in get_all_namespaces_for_context(&f.document, &ctx) {
                if seen.insert(ns.clone()) {
                    namespaces.push(ns);
                }
            }
        }
        namespaces
    }

    /// Check if a namespace exists (is defined in the current context's context entries).
    pub fn namespace_exists(&self, name: &str) -> bool {
        self.get_namespaces().iter().any(|n| n == name)
    }

    /// Save all modified files.
    pub fn save(&self) -> Result<(), KubeconfigError> {
        for f in &self.files {
            f.save()?;
        }
        Ok(())
    }
}

/// Errors from kubeconfig operations.
#[derive(Debug)]
pub enum KubeconfigError {
    Io(String),
    Parse(String),
    Other(String),
}

impl std::fmt::Display for KubeconfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KubeconfigError::Io(s) => write!(f, "{}", s),
            KubeconfigError::Parse(s) => write!(f, "{}", s),
            KubeconfigError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for KubeconfigError {}

/// Resolve the list of kubeconfig file paths from KUBECONFIG env or default.
/// Deduplicates paths so the same file isn't loaded twice.
fn resolve_kubeconfig_paths() -> Vec<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    match env::var(KUBECONFIG_ENV) {
        Ok(val) if !val.is_empty() => {
            let mut seen = std::collections::HashSet::new();
            val
                .split(separator)
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .filter(|p| seen.insert(p.clone()))
                .collect()
        }
        _ => {
            // Default to ~/.kube/config
            if let Some(home) = dirs::home_dir() {
                vec![home.join(".kube").join("config")]
            } else {
                Vec::new()
            }
        }
    }
}

// ---- YAML helper functions ----

/// Get a string field from a YAML mapping.
fn get_field_str(doc: &Value, field: &str) -> Option<String> {
    match doc {
        Value::Mapping(map) => map
            .get(Value::String(field.into()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Set a string field in a YAML mapping.
fn set_field_str(doc: &mut Value, field: &str, value: &str) {
    if let Value::Mapping(ref mut map) = doc {
        map.insert(
            Value::String(field.into()),
            Value::String(value.into()),
        );
    }
}

/// Get all context names from a kubeconfig document.
fn get_context_names(doc: &Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Value::Mapping(map) = doc {
        if let Some(Value::Sequence(seq)) = map.get(Value::String("contexts".into())) {
            for entry in seq {
                if let Value::Mapping(entry_map) = entry {
                    if let Some(Value::String(name)) =
                        entry_map.get(Value::String("name".into()))
                    {
                        names.push(name.clone());
                    }
                }
            }
        }
    }
    names
}

/// Remove a context entry by name. Returns true if removed.
fn remove_context(doc: &mut Value, name: &str) -> bool {
    if let Value::Mapping(ref mut map) = doc {
        if let Some(Value::Sequence(ref mut seq)) =
            map.get_mut(Value::String("contexts".into()))
        {
            let len_before = seq.len();
            seq.retain(|entry| {
                if let Value::Mapping(entry_map) = entry {
                    entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str())
                        != Some(name)
                } else {
                    true
                }
            });
            return seq.len() < len_before;
        }
    }
    false
}

/// Rename a context entry. Returns true if renamed.
fn rename_context_entry(doc: &mut Value, old_name: &str, new_name: &str) -> bool {
    if let Value::Mapping(ref mut map) = doc {
        if let Some(Value::Sequence(ref mut seq)) =
            map.get_mut(Value::String("contexts".into()))
        {
            for entry in seq.iter_mut() {
                if let Value::Mapping(ref mut entry_map) = entry {
                    if entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str())
                        == Some(old_name)
                    {
                        entry_map.insert(
                            Value::String("name".into()),
                            Value::String(new_name.into()),
                        );
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Get the namespace from a specific context entry.
fn get_context_namespace(doc: &Value, context_name: &str) -> Option<String> {
    if let Value::Mapping(map) = doc {
        if let Some(Value::Sequence(seq)) = map.get(Value::String("contexts".into())) {
            for entry in seq {
                if let Value::Mapping(entry_map) = entry {
                    let name = entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str());
                    if name == Some(context_name) {
                        if let Some(Value::Mapping(ctx_map)) =
                            entry_map.get(Value::String("context".into()))
                        {
                            return ctx_map
                                .get(Value::String("namespace".into()))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Get all namespaces defined across context entries for a given context name.
/// In practice, a context name should only appear once, but we scan all files.
fn get_all_namespaces_for_context(doc: &Value, context_name: &str) -> Vec<String> {
    let mut namespaces = Vec::new();
    if let Value::Mapping(map) = doc {
        if let Some(Value::Sequence(seq)) = map.get(Value::String("contexts".into())) {
            for entry in seq {
                if let Value::Mapping(entry_map) = entry {
                    let name = entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str());
                    if name == Some(context_name) {
                        if let Some(Value::Mapping(ctx_map)) =
                            entry_map.get(Value::String("context".into()))
                        {
                            if let Some(ns) =
                                ctx_map.get(Value::String("namespace".into()))
                            {
                                if let Some(s) = ns.as_str() {
                                    namespaces.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    namespaces
}

/// Set the namespace for a specific context entry. Returns true if set.
fn set_context_namespace(doc: &mut Value, context_name: &str, namespace: &str) -> bool {
    if let Value::Mapping(ref mut map) = doc {
        if let Some(Value::Sequence(ref mut seq)) =
            map.get_mut(Value::String("contexts".into()))
        {
            for entry in seq.iter_mut() {
                if let Value::Mapping(ref mut entry_map) = entry {
                    let name = entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if name.as_deref() == Some(context_name) {
                        let ctx_field = entry_map
                            .entry(Value::String("context".into()))
                            .or_insert_with(|| Value::Mapping(serde_yaml::Mapping::new()));
                        if let Value::Mapping(ref mut ctx_map) = ctx_field {
                            ctx_map.insert(
                                Value::String("namespace".into()),
                                Value::String(namespace.into()),
                            );
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Unset the namespace for a specific context entry. Returns true if removed.
fn unset_context_namespace(doc: &mut Value, context_name: &str) -> bool {
    if let Value::Mapping(ref mut map) = doc {
        if let Some(Value::Sequence(ref mut seq)) =
            map.get_mut(Value::String("contexts".into()))
        {
            for entry in seq.iter_mut() {
                if let Value::Mapping(ref mut entry_map) = entry {
                    let name = entry_map
                        .get(Value::String("name".into()))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if name.as_deref() == Some(context_name) {
                        if let Some(Value::Mapping(ref mut ctx_map)) =
                            entry_map.get_mut(Value::String("context".into()))
                        {
                            ctx_map.remove(Value::String("namespace".into()));
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(yaml_str: &str) -> Value {
        serde_yaml::from_str(yaml_str).unwrap()
    }

    #[test]
    fn test_get_context_names() {
        let doc = make_doc(
            r#"
apiVersion: v1
kind: Config
current-context: minikube
contexts:
- name: minikube
  context:
    cluster: minikube
    user: minikube
    namespace: default
- name: gke-prod
  context:
    cluster: gke-prod
    user: gke-prod
    namespace: production
"#,
        );
        let names = get_context_names(&doc);
        assert_eq!(names, vec!["minikube", "gke-prod"]);
    }

    #[test]
    fn test_get_field_str() {
        let doc = make_doc(
            r#"
apiVersion: v1
current-context: minikube
"#,
        );
        assert_eq!(get_field_str(&doc, "current-context"), Some("minikube".to_string()));
        assert_eq!(get_field_str(&doc, "nonexistent"), None);
    }

    #[test]
    fn test_set_field_str() {
        let mut doc = make_doc(
            r#"
apiVersion: v1
current-context: minikube
"#,
        );
        set_field_str(&mut doc, "current-context", "gke-prod");
        assert_eq!(get_field_str(&doc, "current-context"), Some("gke-prod".to_string()));
    }

    #[test]
    fn test_remove_context() {
        let mut doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
- name: gke-prod
  context:
    cluster: gke-prod
"#,
        );
        assert!(remove_context(&mut doc, "minikube"));
        assert!(!remove_context(&mut doc, "minikube"));
        let names = get_context_names(&doc);
        assert_eq!(names, vec!["gke-prod"]);
    }

    #[test]
    fn test_rename_context_entry() {
        let mut doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
"#,
        );
        assert!(rename_context_entry(&mut doc, "minikube", "local"));
        assert!(!rename_context_entry(&mut doc, "nonexistent", "whatever"));
        let names = get_context_names(&doc);
        assert_eq!(names, vec!["local"]);
    }

    #[test]
    fn test_get_context_namespace() {
        let doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
    namespace: default
- name: gke-prod
  context:
    cluster: gke-prod
    namespace: production
"#,
        );
        assert_eq!(
            get_context_namespace(&doc, "minikube"),
            Some("default".to_string())
        );
        assert_eq!(
            get_context_namespace(&doc, "gke-prod"),
            Some("production".to_string())
        );
        assert_eq!(get_context_namespace(&doc, "nonexistent"), None);
    }

    #[test]
    fn test_set_context_namespace() {
        let mut doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
    namespace: default
"#,
        );
        assert!(set_context_namespace(&mut doc, "minikube", "kube-system"));
        assert_eq!(
            get_context_namespace(&doc, "minikube"),
            Some("kube-system".to_string())
        );
        assert!(!set_context_namespace(&mut doc, "nonexistent", "whatever"));
    }

    #[test]
    fn test_unset_context_namespace() {
        let mut doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
    namespace: default
"#,
        );
        assert!(unset_context_namespace(&mut doc, "minikube"));
        assert_eq!(get_context_namespace(&doc, "minikube"), None);
        assert!(!unset_context_namespace(&mut doc, "nonexistent"));
    }

    #[test]
    fn test_get_all_namespaces_for_context() {
        let doc = make_doc(
            r#"
apiVersion: v1
contexts:
- name: minikube
  context:
    cluster: minikube
    namespace: default
"#,
        );
        let namespaces = get_all_namespaces_for_context(&doc, "minikube");
        assert_eq!(namespaces, vec!["default"]);
    }

    #[test]
    fn test_kubeconfig_file_roundtrip() {
        let tmp = std::env::temp_dir().join("kubectx_test_roundtrip.yaml");
        let yaml = r#"apiVersion: v1
kind: Config
current-context: minikube
contexts:
- name: minikube
  context:
    cluster: minikube
    user: minikube
"#;
        std::fs::write(&tmp, yaml).unwrap();
        let file = KubeconfigFile::load(&tmp).unwrap();
        assert_eq!(
            get_field_str(&file.document, "current-context"),
            Some("minikube".to_string())
        );
        file.save().unwrap();
        let reloaded = KubeconfigFile::load(&tmp).unwrap();
        assert_eq!(
            get_field_str(&reloaded.document, "current-context"),
            Some("minikube".to_string())
        );
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_resolve_kubeconfig_paths() {
        // Test KUBECONFIG env var mode
        let saved = std::env::var("KUBECONFIG").ok();
        std::env::set_var("KUBECONFIG", "/tmp/a:/tmp/b:/tmp/c");
        let paths = resolve_kubeconfig_paths();
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], std::path::PathBuf::from("/tmp/a"));
        assert_eq!(paths[1], std::path::PathBuf::from("/tmp/b"));
        assert_eq!(paths[2], std::path::PathBuf::from("/tmp/c"));

        // Test default mode (KUBECONFIG unset)
        std::env::remove_var("KUBECONFIG");
        let paths = resolve_kubeconfig_paths();
        assert!(!paths.is_empty());
        assert!(paths[0].to_string_lossy().contains(".kube"));

        // Restore
        if let Some(v) = saved {
            std::env::set_var("KUBECONFIG", v);
        }
    }
}