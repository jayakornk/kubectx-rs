// Isolated and read-only shell operations.
#![allow(dead_code)]
//
// `kubectx -s <NAME>` starts a shell scoped to a single context by creating
// a temporary kubeconfig containing only that context and setting KUBECONFIG.
//
// `kubectx -r <NAME>` does the same but also blocks write operations by
// installing a kubectl wrapper that rejects mutating subcommands.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use colored::Colorize;

use crate::kubeconfig;

/// Start an isolated shell scoped to a single context.
///
/// Creates a temp kubeconfig with only the target context, sets KUBECONFIG,
/// and spawns the user's $SHELL. The temp file is cleaned up on exit.
pub fn start_shell(context: &str, readonly: bool) -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;

    if !kc.context_exists(context) {
        return Err(format!("no context exists with name \"{}\"", context));
    }

    // Create a temp directory for the isolated kubeconfig
    let tmpdir = mktemp_dir().map_err(|e| format!("failed to create temp dir: {}", e))?;
    let kubeconfig_path = tmpdir.join("config");

    // Extract the single context into a new kubeconfig
    let temp_kubeconfig = extract_context_kubeconfig(&kc, context)
        .map_err(|e| format!("failed to extract context: {}", e))?;

    fs::write(&kubeconfig_path, &temp_kubeconfig)
        .map_err(|e| format!("failed to write temp kubeconfig: {}", e))?;

    // Determine the shell to use
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // Build the environment for the sub-shell
    let mut shell_env: Vec<(String, String)> = Vec::new();

    // Set KUBECONFIG to the temp file
    shell_env.push((
        "KUBECONFIG".to_string(),
        kubeconfig_path.to_string_lossy().to_string(),
    ));

    // Set a prompt that shows the context name
    let prompt_prefix = if readonly {
        format!("{} (ro) ", context)
    } else {
        format!("{} ", context)
    };
    shell_env.push(("KUBECTX_SHELL_CONTEXT".to_string(), context.to_string()));
    shell_env.push(("PS1".to_string(), format!("[{}] \\w $ ", prompt_prefix)));

    // For readonly mode, install a kubectl wrapper
    if readonly {
        shell_env.push(("KUBECTX_READONLY".to_string(), "1".to_string()));

        // Create a bin directory with a kubectl wrapper
        let bindir = tmpdir.join("bin");
        fs::create_dir_all(&bindir).map_err(|e| format!("failed to create bin dir: {}", e))?;

        let kubectl_path = which_kubectl().unwrap_or_else(|| "kubectl".to_string());
        let wrapper = format!(
            r#"#!/bin/bash
# kubectl wrapper — blocks write operations in read-only shell
if [ "$KUBECTX_READONLY" = "1" ]; then
  case "$1" in
    create|apply|delete|edit|patch|replace|scale|rollout|label|annotate|\
    taint|cordon|uncordon|drain|exec|port-forward|proxy|run|set|expose|\
    autoscale|cp|attach|auth|adm)
      echo "{0} write operations are blocked in read-only shell" >&2
      exit 1
      ;;
  esac
fi
exec {1} "$@"
"#,
            "error:".red(),
            kubectl_path
        );

        let wrapper_path = bindir.join("kubectl");
        let mut file = fs::File::create(&wrapper_path)
            .map_err(|e| format!("failed to create kubectl wrapper: {}", e))?;
        file.write_all(wrapper.as_bytes())
            .map_err(|e| format!("failed to write kubectl wrapper: {}", e))?;
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("failed to chmod wrapper: {}", e))?;
        }

        // Prepend the bin dir to PATH
        let current_path = env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bindir.to_string_lossy(), current_path);
        shell_env.push(("PATH".to_string(), new_path));
    }

    // Print a message about the shell
    if readonly {
        eprintln!(
            "{} Started read-only shell for context \"{}\". Exit to return.",
            "→".cyan(),
            context.cyan()
        );
    } else {
        eprintln!(
            "{} Started shell for context \"{}\". Exit to return.",
            "→".cyan(),
            context.cyan()
        );
    }

    // Spawn the shell
    let mut cmd = Command::new(&shell);
    cmd.env_clear();
    // Inherit essential env vars
    for var in &[
        "TERM", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR", "TMP", "TEMP",
    ] {
        if let Ok(val) = env::var(var) {
            cmd.env(var, val);
        }
    }
    // Apply our custom env
    for (key, val) in &shell_env {
        cmd.env(key, val);
    }
    // Inherit PATH unless readonly already set it
    if !readonly {
        if let Ok(path) = env::var("PATH") {
            cmd.env("PATH", path);
        }
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to spawn shell: {}", e))?;

    // Cleanup
    let _ = fs::remove_dir_all(&tmpdir);

    if !status.success() {
        return Err(format!(
            "shell exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

/// Extract a single context (with its cluster and user) into a standalone
/// kubeconfig YAML string.
fn extract_context_kubeconfig(
    kc: &kubeconfig::Kubeconfig,
    context: &str,
) -> Result<String, String> {
    // We need to find the context entry, its cluster, and its user,
    // then build a new kubeconfig with only those entries.
    let yaml = serde_yaml_ng::to_string(
        kc.files()
            .first()
            .map(|f| &f.document)
            .ok_or("no kubeconfig files")?,
    )
    .map_err(|e| format!("failed to serialize: {}", e))?;

    // Parse the full kubeconfig
    let doc: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&yaml).map_err(|e| format!("failed to parse: {}", e))?;

    // Find the context entry
    let contexts = doc.get("contexts").and_then(|v| v.as_sequence());
    let context_entry = contexts
        .and_then(|seq| {
            seq.iter()
                .find(|e| e.get("name").and_then(|n| n.as_str()) == Some(context))
        })
        .ok_or_else(|| format!("context \"{}\" not found", context))?;

    let cluster_name = context_entry
        .get("context")
        .and_then(|c| c.get("cluster"))
        .and_then(|c| c.as_str())
        .ok_or("no cluster in context")?;
    let user_name = context_entry
        .get("context")
        .and_then(|c| c.get("user"))
        .and_then(|u| u.as_str())
        .ok_or("no user in context")?;

    // Build a new kubeconfig
    let mut new_doc = serde_yaml_ng::Mapping::new();
    new_doc.insert(
        serde_yaml_ng::Value::String("apiVersion".into()),
        serde_yaml_ng::Value::String("v1".into()),
    );
    new_doc.insert(
        serde_yaml_ng::Value::String("kind".into()),
        serde_yaml_ng::Value::String("Config".into()),
    );
    new_doc.insert(
        serde_yaml_ng::Value::String("current-context".into()),
        serde_yaml_ng::Value::String(context.into()),
    );

    // Add the context entry
    let mut ctx_seq = serde_yaml_ng::Sequence::new();
    ctx_seq.push(context_entry.clone());
    new_doc.insert(
        serde_yaml_ng::Value::String("contexts".into()),
        serde_yaml_ng::Value::Sequence(ctx_seq),
    );

    // Add the matching cluster
    if let Some(clusters) = doc.get("clusters").and_then(|v| v.as_sequence()) {
        if let Some(cluster_entry) = clusters
            .iter()
            .find(|e| e.get("name").and_then(|n| n.as_str()) == Some(cluster_name))
        {
            let mut cluster_seq = serde_yaml_ng::Sequence::new();
            cluster_seq.push(cluster_entry.clone());
            new_doc.insert(
                serde_yaml_ng::Value::String("clusters".into()),
                serde_yaml_ng::Value::Sequence(cluster_seq),
            );
        }
    }

    // Add the matching user
    if let Some(users) = doc.get("users").and_then(|v| v.as_sequence()) {
        if let Some(user_entry) = users
            .iter()
            .find(|e| e.get("name").and_then(|n| n.as_str()) == Some(user_name))
        {
            let mut user_seq = serde_yaml_ng::Sequence::new();
            user_seq.push(user_entry.clone());
            new_doc.insert(
                serde_yaml_ng::Value::String("users".into()),
                serde_yaml_ng::Value::Sequence(user_seq),
            );
        }
    }

    serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(new_doc))
        .map_err(|e| format!("failed to serialize temp kubeconfig: {}", e))
}

/// Find the full path to kubectl.
fn which_kubectl() -> Option<String> {
    let result = Command::new("which")
        .arg("kubectl")
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

/// Create a temporary directory.
fn mktemp_dir() -> io::Result<PathBuf> {
    let tmp = env::temp_dir();
    let mut name = format!("kubectx-shell-");
    name.push_str(&format!("{}", std::process::id()));
    name.push_str(&format!(
        "-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let path = tmp.join(name);
    fs::create_dir(&path)?;
    Ok(path)
}

use std::process::Stdio;
