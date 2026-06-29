// kubens(1) – Faster way to switch between namespaces in kubectl.
//
// Usage:
//   kubens                        : list the namespaces
//   kubens <NAME>                 : switch to namespace <NAME>
//   kubens -                      : switch to the previous namespace
//   kubens -c, --current          : show the current namespace name
//   kubens -u, --unset            : unset the current namespace
//   kubens <NEW_NAME>=<NAME>      : rename namespace <NAME> to <NEW_NAME>
//   kubens -f, --force <NAME>     : switch even if namespace doesn't exist
//   kubens -d <NAME> [<NAME...>]  : delete namespace <NAME> ('.' for current)
//   kubens --dry-run              : show what would change without writing
//   kubens -o, --output json      : JSON output for list
//   kubens completion <shell>     : print completion script (bash/zsh/fish)
//   kubens __complete             : (hidden) print namespace names for completion
//   kubens -h, --help             : show this message
//   kubens -V, --version          : show version

#[path = "kubeconfig.rs"]
mod kubeconfig;
#[path = "printer.rs"]
mod printer;
#[path = "fzf.rs"]
mod fzf;
#[path = "state.rs"]
mod state;
#[path = "alias.rs"]
mod alias;
#[path = "health.rs"]
mod health;
#[path = "shell.rs"]
mod shell;
#[path = "completion.rs"]
mod completion;

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
  kubens -f, --force <NAME>     : switch even if namespace doesn't exist
  kubens -d <NAME> [<NAME...>]  : delete namespace <NAME> ('.' for current)
  kubens --dry-run              : show what would change without writing
  kubens -o, --output json      : JSON output for list
  kubens completion <shell>     : print completion script (bash/zsh/fish)
  kubens -h, --help             : show this message
  kubens -V, --version          : show version"#;

/// Global flags extracted from args.
struct GlobalFlags {
    dry_run: bool,
    output_json: bool,
    force: bool,
}

impl GlobalFlags {
    fn extract(args: &mut Vec<String>) -> Self {
        let mut flags = GlobalFlags {
            dry_run: false,
            output_json: false,
            force: false,
        };
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--dry-run" => {
                    flags.dry_run = true;
                    args.remove(i);
                }
                "-f" | "--force" => {
                    flags.force = true;
                    args.remove(i);
                }
                "-o" | "--output" => {
                    if i + 1 < args.len() && args[i + 1] == "json" {
                        flags.output_json = true;
                        args.remove(i);
                        args.remove(i);
                    } else {
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }
        flags
    }
}

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
    let mut args = args.to_vec();
    let flags = GlobalFlags::extract(&mut args);

    if args.is_empty() {
        // Skip fzf if flags explicitly request a list view (--output json).
        if flags.output_json {
            return op_list(&flags);
        }
        if printer::is_interactive() && fzf::fzf_available() {
            return op_interactive_switch();
        }
        return op_list(&flags);
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
            return op_unset(&flags);
        }
        "-d" | "--delete" => {
            return op_delete(&args[1..], &flags);
        }
        "completion" => {
            let shell = args.get(1).map(|s| s.as_str()).unwrap_or("bash");
            println!("{}", completion::generate("kubens", shell));
            return Ok(());
        }
        "__complete" => {
            // Hidden subcommand for shell completion
            let kc = kubeconfig::Kubeconfig::load_default()
                .map_err(|e| format!("kubeconfig error: {}", e))?;
            // For completion, try cluster namespaces first
            match query_cluster_namespaces() {
                Some(ns) => {
                    for n in ns {
                        println!("{}", n);
                    }
                }
                None => {
                    for n in kc.get_namespaces() {
                        println!("{}", n);
                    }
                }
            }
            return Ok(());
        }
        _ => {}
    }

    if arg == "-" {
        return op_swap(&flags);
    }

    // "<NEW>=<OLD>" → rename
    if let Some(eq_pos) = arg.find('=') {
        let new_name = &arg[..eq_pos];
        let old_name = &arg[eq_pos + 1..];
        return op_rename(new_name, old_name, &flags);
    }

    // "<NAME>" → switch namespace
    op_switch(arg, &flags)
}

/// List all namespaces in the current context.
/// Queries the live Kubernetes API via kubectl to get the full list of namespaces.
fn op_list(flags: &GlobalFlags) -> Result<(), String> {
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

    // JSON output
    if flags.output_json {
        let entries: Vec<JsonEntry> = namespaces
            .iter()
            .map(|n| JsonEntry {
                name: n.clone(),
                current: current_ns.as_deref() == Some(n.as_str()),
            })
            .collect();
        let mut items = Vec::new();
        for e in &entries {
            items.push(format!(
                r#"{{"name":"{}","current":{}}}"#,
                e.name.replace('"', "\\\""),
                e.current
            ));
        }
        println!("[{}]", items.join(","));
        return Ok(());
    }

    printer::print_namespace_list(&namespaces, current_ns.as_deref());
    Ok(())
}

/// Query the Kubernetes API via `kubectl get namespaces` for the full list
/// of namespaces in the current cluster.
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
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut namespaces = Vec::new();
    for line in stdout.lines() {
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
fn op_switch(name: &str, flags: &GlobalFlags) -> Result<(), String> {
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

    // Validate namespace exists unless --force
    if !flags.force {
        if let Some(cluster_ns) = query_cluster_namespaces() {
            if !cluster_ns.iter().any(|n| n == name) {
                eprintln!(
                    "{} namespace \"{}\" does not exist in the cluster. Use -f to force.",
                    "warning:".yellow(),
                    name
                );
                return Err(format!("namespace \"{}\" not found (use --force to override)", name));
            }
        }
    }

    if flags.dry_run {
        eprintln!("{} Would switch to namespace \"{}\"", "dry-run:".yellow(), name.cyan());
        return Ok(());
    }

    kc.set_current_namespace(name)
        .map_err(|e| format!("failed to set namespace: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_switched_namespace(name);
    Ok(())
}

/// Swap to the previous namespace.
fn op_swap(flags: &GlobalFlags) -> Result<(), String> {
    let prev_file = state::prev_namespace_file()
        .ok_or_else(|| "failed to determine state file path".to_string())?;
    let prev = state::read_state(&prev_file)
        .map_err(|e| format!("failed to read previous namespace file: {}", e))?;
    if prev.is_empty() {
        return Err("no previous namespace found".into());
    }
    op_switch(&prev, flags)
}

/// Unset the current namespace.
fn op_unset(flags: &GlobalFlags) -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

    if flags.dry_run {
        eprintln!("{} Would unset namespace", "dry-run:".yellow());
        return Ok(());
    }

    kc.unset_current_namespace()
        .map_err(|e| format!("failed to unset namespace: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success("Namespace unset");
    Ok(())
}

/// Rename a namespace.
fn op_rename(new_name: &str, old_name: &str, flags: &GlobalFlags) -> Result<(), String> {
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

    if flags.dry_run {
        eprintln!("{} Would rename namespace \"{}\" → \"{}\"", "dry-run:".yellow(), old_name, new_name.cyan());
        return Ok(());
    }

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
fn op_delete(names: &[String], flags: &GlobalFlags) -> Result<(), String> {
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

    if flags.dry_run {
        eprintln!("{} Would save changes to kubeconfig", "dry-run:".yellow());
        return Ok(());
    }

    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    Ok(())
}

/// Interactive switch with fzf.
/// Opens fzf immediately, then queries `kubectl get namespaces` in a
/// background thread. Items stream in as the cluster responds.
fn op_interactive_switch() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let current = kc.get_current_namespace();

    let selected = fzf::fuzzy_select_streaming(
        current.as_deref(),
        || {
            match query_cluster_namespaces() {
                Some(ns) if !ns.is_empty() => ns,
                _ => {
                    let kc = kubeconfig::Kubeconfig::load_default().unwrap();
                    kc.get_namespaces()
                }
            }
        },
    )
    .ok_or_else(|| "no namespace selected".to_string())?;
    op_switch(&selected, &GlobalFlags { dry_run: false, output_json: false, force: true })
}

struct JsonEntry {
    name: String,
    current: bool,
}