// kubens(1) – Faster way to switch between namespaces in kubectl.
//
// Usage:
//   kubens                        : list the namespaces
//   kubens <NAME>                 : switch to namespace <NAME>
//   kubens -                      : switch to the previous namespace
//   kubens -c, --current          : show the current namespace name
//   kubens -u, --unset            : unset the current namespace
//   kubens <NEW_NAME>=<NAME>      : rename namespace <NAME> to <NEW_NAME>
//   kubens -d <NAME> [<NAME...>]  : delete namespace <NAME> ('.' for current-namespace)
//   kubens -h, --help             : show this message
//   kubens -V, --version          : show version

mod kubeconfig;
#[path = "printer.rs"]
mod printer;
#[path = "fzf.rs"]
mod fzf;
#[path = "state.rs"]
mod state;

use std::env;
use std::process::ExitCode;

use colored::Colorize;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP_TEXT: &str = r#"USAGE:
  kubens                        : list the namespaces
  kubens <NAME>                 : switch to namespace <NAME>
  kubens -                      : switch to the previous namespace
  kubens -c, --current          : show the current namespace name
  kubens -u, --unset            : unset the current namespace
  kubens <NEW_NAME>=<NAME>      : rename namespace <NAME> to <NEW_NAME>
  kubens -d <NAME> [<NAME...>]  : delete namespace <NAME> ('.' for current-namespace)
  kubens -h, --help             : show this message
  kubens -V, --version          : show version"#;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            printer::print_error(&e);
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        if printer::is_interactive() && fzf::fzf_available() {
            return op_interactive_switch();
        }
        return op_list();
    }

    let arg = &args[0];

    match arg.as_str() {
        "-h" | "--help" => {
            println!("{}", HELP_TEXT);
            return Ok(());
        }
        "-V" | "--version" => {
            println!("kubens {}", VERSION);
            return Ok(());
        }
        "-c" | "--current" => {
            return op_current();
        }
        "-u" | "--unset" => {
            return op_unset();
        }
        "-d" | "--delete" => {
            return op_delete(&args[1..]);
        }
        _ => {}
    }

    if arg == "-" {
        return op_swap();
    }

    // "<NEW>=<OLD>" → rename
    if let Some(eq_pos) = arg.find('=') {
        let new_name = &arg[..eq_pos];
        let old_name = &arg[eq_pos + 1..];
        return op_rename(new_name, old_name);
    }

    // "<NAME>" → switch namespace
    op_switch(arg)
}

/// List all namespaces in the current context.
/// Queries the live Kubernetes API via kubectl to get the full list of namespaces.
fn op_list() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let current_ctx = kc.get_current_context().ok_or_else(|| {
        "no current context set; use kubectx to select a context first".to_string()
    })?;
    let current_ns = kc.get_current_namespace();

    // Try querying the live cluster for all namespaces
    let namespaces = match query_cluster_namespaces() {
        Some(ns) if !ns.is_empty() => ns,
        _ => {
            // Fall back to kubeconfig-defined namespaces
            let ns = kc.get_namespaces();
            if ns.is_empty() {
                return Err(format!(
                    "no namespaces found for context \"{}\"\n\
                     hint: make sure kubectl can reach the cluster",
                    current_ctx
                ));
            }
            ns
        }
    };

    printer::print_namespace_list(&namespaces, current_ns.as_deref());
    Ok(())
}

/// Query the Kubernetes API via `kubectl get namespaces` for the full list
/// of namespaces in the current cluster.
/// Returns None if kubectl is unavailable or the cluster is unreachable.
fn query_cluster_namespaces() -> Option<Vec<String>> {
    let output = std::process::Command::new("kubectl")
        .arg("get")
        .arg("namespaces")
        .arg("-o")
        .arg("name")
        .arg("--no-headers")
        .output()
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("{} {}", "warning:".yellow(), stderr.trim());
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut namespaces = Vec::new();
    for line in stdout.lines() {
        // kubectl -o name outputs "namespace/<name>"
        let name = line.strip_prefix("namespace/").unwrap_or(line);
        if !name.is_empty() {
            namespaces.push(name.to_string());
        }
    }
    namespaces.sort();
    Some(namespaces)
}

/// Show the current namespace.
fn op_current() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    match kc.get_current_namespace() {
        Some(ns) => {
            println!("{}", ns);
            Ok(())
        }
        None => Err("no namespace set for the current context".into()),
    }
}

/// Switch to a namespace.
fn op_switch(name: &str) -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

    // Save current namespace as "previous" before switching
    if let Some(current) = kc.get_current_namespace() {
        if current != name {
            if let Some(prev_file) = state::prev_namespace_file() {
                let _ = state::write_state(&prev_file, &current);
            }
        }
    }

    kc.set_current_namespace(name)
        .map_err(|e| format!("failed to set namespace: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_switched_namespace(name);
    Ok(())
}

/// Swap to the previous namespace.
fn op_swap() -> Result<(), String> {
    let prev_file = state::prev_namespace_file()
        .ok_or_else(|| "failed to determine state file path".to_string())?;
    let prev = state::read_state(&prev_file)
        .map_err(|e| format!("failed to read previous namespace file: {}", e))?;
    if prev.is_empty() {
        return Err("no previous namespace found".into());
    }
    op_switch(&prev)
}

/// Unset the current namespace.
fn op_unset() -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    kc.unset_current_namespace()
        .map_err(|e| format!("failed to unset namespace: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success("Namespace unset");
    Ok(())
}

/// Rename a namespace.
fn op_rename(new_name: &str, old_name: &str) -> Result<(), String> {
    // For namespaces, rename means changing the namespace field in the current context.
    // We need the current context to find and rename its namespace.
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

    let old_name = if old_name == "." {
        kc.get_current_namespace()
            .ok_or_else(|| "no current namespace to rename".to_string())?
    } else {
        old_name.to_string()
    };

    if new_name == old_name {
        return Err("new name and old name are the same".into());
    }

    // For kubens, rename changes the namespace of the current context
    kc.set_current_namespace(new_name)
        .map_err(|e| format!("failed to rename namespace: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success(&format!(
        "Namespace \"{}\" renamed to \"{}\".",
        old_name, new_name
    ));
    Ok(())
}

/// Delete one or more namespaces.
/// For kubens, this means removing the namespace field from context entries.
fn op_delete(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("specify at least one namespace to delete".into());
    }

    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let current_ns = kc.get_current_namespace();

    for name in names {
        let target = if name == "." {
            current_ns
                .clone()
                .ok_or_else(|| "no current namespace to delete".to_string())?
        } else {
            name.clone()
        };

        // For kubens, "delete" means removing the namespace from the current context
        // if it matches, or just acknowledging it.
        if current_ns.as_deref() == Some(target.as_str()) {
            kc.unset_current_namespace()
                .map_err(|e| format!("failed to delete namespace: {}", e))?;
            printer::print_info(&format!("Deleted namespace \"{}\".", target));
        } else {
            return Err(format!(
                "namespace \"{}\" is not the current namespace; only the current namespace can be deleted",
                target
            ));
        }
    }

    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    Ok(())
}

/// Interactive switch with fzf.
/// Queries the live cluster for the full namespace list when available.
fn op_interactive_switch() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

    // Try the live cluster first, fall back to kubeconfig
    let namespaces = match query_cluster_namespaces() {
        Some(ns) if !ns.is_empty() => ns,
        _ => {
            let ns = kc.get_namespaces();
            if ns.is_empty() {
                return Err("no namespaces found in current context".into());
            }
            ns
        }
    };
    let current = kc.get_current_namespace();
    let selected = fzf::fuzzy_select(&namespaces, current.as_deref())
        .ok_or_else(|| "no namespace selected".to_string())?;
    op_switch(&selected)
}