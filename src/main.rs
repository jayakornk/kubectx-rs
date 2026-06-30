// kubectx(1) – Faster way to switch between clusters in kubectl.
//
// Usage:
//   kubectx                       : list the contexts
//   kubectx <NAME>                : switch to context <NAME>
//   kubectx -                     : switch to the previous context
//   kubectx -c, --current         : show the current context name
//   kubectx -u, --unset           : unset the current context
//   kubectx <NEW_NAME>=<NAME>     : rename context <NAME> to <NEW_NAME>
//   kubectx <NEW_NAME>=.          : rename current-context to <NEW_NAME>
//   kubectx -d <NAME> [<NAME...>] : delete context <NAME> ('.' for current-context)
//   kubectx -s, --shell <NAME>    : start a shell scoped to context <NAME>
//   kubectx -r, --readonly <NAME> : start a read-only shell for context <NAME>
//   kubectx -i, --info <NAME>     : show context details ('.' for current)
//   kubectx --health              : list with cluster health indicators
//   kubectx --dry-run             : show what would change without writing
//   kubectx -o, --output json     : JSON output for list
//   kubectx @<alias>              : switch by alias
//   kubectx @<alias>=<context>    : set alias
//   kubectx --aliases             : list all aliases
//   kubectx completion <shell>    : print completion script (bash/zsh/fish)
//   kubectx __complete            : (hidden) print context names for completion
//   kubectx -h, --help            : show this message
//   kubectx -V, --version         : show version

mod alias;
mod completion;
mod fzf;
mod health;
mod kubeconfig;
mod printer;
mod shell;
mod state;

use std::env;
use std::process::ExitCode;

use colored::Colorize;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP_TEXT: &str = r#"USAGE:
  kubectx                       : list the contexts
  kubectx <NAME>                : switch to context <NAME>
  kubectx -                     : switch to the previous context
  kubectx -c, --current         : show the current context name
  kubectx -u, --unset           : unset the current context
  kubectx <NEW_NAME>=<NAME>     : rename context <NAME> to <NEW_NAME>
  kubectx <NEW_NAME>=.          : rename current-context to <NEW_NAME>
  kubectx -d <NAME> [<NAME...>] : delete context <NAME> ('.' for current-context)
  kubectx -s, --shell <NAME>    : start a shell scoped to context <NAME>
  kubectx -r, --readonly <NAME> : start a read-only shell for context <NAME>
  kubectx -i, --info <NAME>     : show context details ('.' for current)
  kubectx --health              : list with cluster health indicators
  kubectx --dry-run             : show what would change without writing
  kubectx -o, --output json     : JSON output for list
  kubectx @<alias>              : switch by alias
  kubectx @<alias>=<context>    : set alias
  kubectx --aliases             : list all aliases
  kubectx completion <shell>    : print completion script (bash/zsh/fish)
  kubectx -h, --help            : show this message
  kubectx -V, --version         : show version"#;

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

/// Global flags extracted from args, removed before operation dispatch.
struct GlobalFlags {
    dry_run: bool,
    output_json: bool,
    show_health: bool,
}

impl GlobalFlags {
    fn extract(args: &mut Vec<String>) -> Self {
        let mut flags = GlobalFlags {
            dry_run: false,
            output_json: false,
            show_health: false,
        };
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--dry-run" => {
                    flags.dry_run = true;
                    args.remove(i);
                }
                "--health" => {
                    flags.show_health = true;
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

fn run(args: &[String]) -> Result<(), String> {
    let mut args = args.to_vec();
    let flags = GlobalFlags::extract(&mut args);

    if args.is_empty() {
        // No positional args: list contexts or interactive fzf mode.
        // But skip fzf if flags explicitly request a list view (--health, --output json).
        if flags.show_health || flags.output_json {
            return op_list(&flags);
        }
        if printer::is_interactive() && fzf::fzf_available() {
            return op_interactive_switch();
        }
        return op_list(&flags);
    }

    let arg = &args[0];

    // Flags
    match arg.as_str() {
        "-h" | "--help" => {
            println!("{}", HELP_TEXT);
            return Ok(());
        }
        "-V" | "--version" => {
            println!("kubectx {}", VERSION);
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
        "-s" | "--shell" => {
            let target = args.get(1).map(|s| s.as_str());
            return op_shell(target, false);
        }
        "-r" | "--readonly" => {
            let target = args.get(1).map(|s| s.as_str());
            return op_shell(target, true);
        }
        "-i" | "--info" => {
            let target = args.get(1).map(|s| s.as_str()).unwrap_or(".");
            return op_info(target);
        }
        "--aliases" => {
            return op_list_aliases();
        }
        "completion" => {
            let shell = args.get(1).map(|s| s.as_str()).unwrap_or("bash");
            println!("{}", completion::generate("kubectx", shell));
            return Ok(());
        }
        "__complete" => {
            // Hidden subcommand for shell completion
            let kc = kubeconfig::Kubeconfig::load_default()
                .map_err(|e| format!("kubeconfig error: {}", e))?;
            for ctx in kc.get_contexts() {
                println!("{}", ctx);
            }
            return Ok(());
        }
        _ => {}
    }

    // "-" → swap to previous context
    if arg == "-" {
        return op_swap(&flags);
    }

    // Alias handling: @alias or @alias=context
    if alias::is_alias_ref(arg) {
        let inner = alias::strip_at(arg);
        if let Some(eq) = inner.find('=') {
            // Set alias: @alias=context
            let alias_name = &inner[..eq];
            let ctx_name = &inner[eq + 1..];
            return op_set_alias(alias_name, ctx_name);
        } else {
            // Switch by alias: @alias
            return op_switch_by_alias(inner, &flags);
        }
    }

    // "<NEW>=<OLD>" → rename
    if let Some(eq_pos) = arg.find('=') {
        let new_name = &arg[..eq_pos];
        let old_name = &arg[eq_pos + 1..];
        return op_rename(new_name, old_name, &flags);
    }

    // "<NAME>" → switch to context
    op_switch(arg, &flags)
}

/// List all available contexts.
fn op_list(flags: &GlobalFlags) -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    let contexts = kc.get_contexts();
    let current = kc.get_current_context();
    if contexts.is_empty() {
        return Err("no contexts found in kubeconfig".into());
    }

    // JSON output
    if flags.output_json {
        let entries: Vec<serde_json::Entry> = contexts
            .iter()
            .map(|c| serde_json::Entry {
                name: c.clone(),
                current: current.as_deref() == Some(c.as_str()),
            })
            .collect();
        println!("{}", serde_json::to_string(&entries));
        return Ok(());
    }

    // Health indicators
    if flags.show_health {
        let health_map = health::check_all_health(&contexts);
        printer::print_context_list_with_health(&contexts, current.as_deref(), &health_map);
        return Ok(());
    }

    printer::print_context_list(&contexts, current.as_deref());
    Ok(())
}

/// Show the current context name.
fn op_current() -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    match kc.get_current_context() {
        Some(ctx) => {
            println!("{}", ctx);
            Ok(())
        }
        None => Err("current-context is not set".into()),
    }
}

/// Switch to a context by name.
fn op_switch(name: &str, flags: &GlobalFlags) -> Result<(), String> {
    let mut kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;

    if !kc.context_exists(name) {
        return Err(format!("no context exists with name \"{}\"", name));
    }

    // Save the current context as "previous" before switching
    if let Some(current) = kc.get_current_context() {
        if current != name {
            if let Some(prev_file) = state::prev_context_file() {
                let _ = state::write_state(&prev_file, &current);
            }
        }
    }

    if flags.dry_run {
        eprintln!(
            "{} Would switch to context \"{}\"",
            "dry-run:".yellow(),
            name.cyan()
        );
        return Ok(());
    }

    kc.set_current_context(name)
        .map_err(|e| format!("failed to set current context: {}", e))?;
    kc.save()
        .map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_switched_context(name);
    Ok(())
}

/// Switch to a context by alias.
fn op_switch_by_alias(alias_name: &str, flags: &GlobalFlags) -> Result<(), String> {
    match alias::resolve_alias(alias_name) {
        Some(ctx) => op_switch(&ctx, flags),
        None => Err(format!(
            "no alias \"@{}\" found. Set one with: kubectx @{}=<context>",
            alias_name, alias_name
        )),
    }
}

/// Set an alias.
fn op_set_alias(alias_name: &str, ctx_name: &str) -> Result<(), String> {
    // Validate that the context exists
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    if !kc.context_exists(ctx_name) {
        return Err(format!("no context exists with name \"{}\"", ctx_name));
    }

    alias::set_alias(alias_name, ctx_name).map_err(|e| format!("failed to write alias: {}", e))?;
    printer::print_success(&format!(
        "Alias \"@{}\" → \"{}\" set.",
        alias_name, ctx_name
    ));
    Ok(())
}

/// List all aliases.
fn op_list_aliases() -> Result<(), String> {
    let aliases = alias::load_aliases();
    if aliases.is_empty() {
        eprintln!("No aliases set. Use: kubectx @<alias>=<context>");
        return Ok(());
    }
    for (alias, ctx) in &aliases {
        println!("  {} {}", format!("@{}", alias).cyan().bold(), ctx);
    }
    Ok(())
}

/// Swap to the previous context.
fn op_swap(flags: &GlobalFlags) -> Result<(), String> {
    let prev_file = state::prev_context_file()
        .ok_or_else(|| "failed to determine state file path".to_string())?;
    let prev = state::read_state(&prev_file)
        .map_err(|e| format!("failed to read previous context file: {}", e))?;
    if prev.is_empty() {
        return Err("no previous context found".into());
    }
    op_switch(&prev, flags)
}

/// Unset the current context.
fn op_unset(flags: &GlobalFlags) -> Result<(), String> {
    let mut kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;

    if flags.dry_run {
        eprintln!("{} Would unset current context", "dry-run:".yellow());
        return Ok(());
    }

    kc.unset_current_context()
        .map_err(|e| format!("failed to unset current context: {}", e))?;
    kc.save()
        .map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success("Context unset");
    Ok(())
}

/// Rename a context.
fn op_rename(new_name: &str, old_name: &str, flags: &GlobalFlags) -> Result<(), String> {
    let mut kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    kc.rename_context(new_name, old_name)
        .map_err(|e| format!("{}", e))?;

    if flags.dry_run {
        eprintln!(
            "{} Would rename \"{}\" → \"{}\"",
            "dry-run:".yellow(),
            old_name,
            new_name.cyan()
        );
        return Ok(());
    }

    kc.save()
        .map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success(&format!(
        "Context \"{}\" renamed to \"{}\".",
        old_name, new_name
    ));
    Ok(())
}

/// Delete one or more contexts.
fn op_delete(names: &[String], flags: &GlobalFlags) -> Result<(), String> {
    if names.is_empty() {
        // Interactive deletion with fzf
        if printer::is_interactive() && fzf::fzf_available() {
            return op_delete_interactive();
        }
        return Err("specify at least one context to delete".into());
    }

    let mut kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;

    for name in names {
        let target = if name == "." {
            kc.get_current_context()
                .ok_or_else(|| "no current context to delete".to_string())?
        } else if alias::is_alias_ref(name) {
            // Delete alias: -d @prod
            let alias_name = alias::strip_at(name);
            let existed = alias::delete_alias(alias_name)
                .map_err(|e| format!("failed to delete alias: {}", e))?;
            if !existed {
                return Err(format!("no alias \"@{}\" found", alias_name));
            }
            printer::print_info(&format!("Deleted alias \"@{}\".", alias_name));
            continue;
        } else {
            name.clone()
        };

        let (was_active, was_found) = kc.delete_context(&target);
        if !was_found {
            return Err(format!("no context found with name \"{}\"", target));
        }
        printer::print_info(&format!("Deleted context \"{}\".", target));
        if was_active {
            printer::print_info("Active context deleted, unset the current-context.");
        }
    }

    if flags.dry_run {
        eprintln!("{} Would save changes to kubeconfig", "dry-run:".yellow());
        return Ok(());
    }

    kc.save()
        .map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    Ok(())
}

/// Interactive deletion with fzf.
fn op_delete_interactive() -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    let current = kc.get_current_context();

    let selected = fzf::fuzzy_select_streaming(current.as_deref(), || {
        let kc = kubeconfig::Kubeconfig::load_default().unwrap();
        kc.get_contexts()
    })
    .ok_or_else(|| "no context selected".to_string())?;
    op_delete(
        &[selected],
        &GlobalFlags {
            dry_run: false,
            output_json: false,
            show_health: false,
        },
    )
}

/// Start an isolated or read-only shell.
fn op_shell(target: Option<&str>, readonly: bool) -> Result<(), String> {
    let context = match target {
        Some(name) => name.to_string(),
        None => {
            // Interactive selection with fzf
            if !printer::is_interactive() || !fzf::fzf_available() {
                return Err("specify a context name or run in a terminal with fzf".into());
            }
            let kc = kubeconfig::Kubeconfig::load_default()
                .map_err(|e| format!("kubeconfig error: {}", e))?;
            let current = kc.get_current_context();
            fzf::fuzzy_select_streaming(current.as_deref(), || {
                let kc = kubeconfig::Kubeconfig::load_default().unwrap();
                kc.get_contexts()
            })
            .ok_or_else(|| "no context selected".to_string())?
        }
    };
    shell::start_shell(&context, readonly)
}

/// Show context info.
fn op_info(target: &str) -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;

    let context = if target == "." {
        kc.get_current_context()
            .ok_or_else(|| "no current context set".to_string())?
    } else {
        target.to_string()
    };

    let info = kc
        .get_context_info(&context)
        .ok_or_else(|| format!("context \"{}\" not found", context))?;

    let is_current = kc.get_current_context().as_deref() == Some(context.as_str());

    println!(
        "{} {}",
        "Context:".cyan().bold(),
        if is_current {
            format!("{} {}", context, "(current)".green())
        } else {
            context.clone()
        }
    );
    println!("{} {}", "  Cluster:".cyan(), info.cluster);
    if let Some(server) = &info.cluster_server {
        println!("{} {}", "  Server:".cyan(), server);
    }
    println!("{} {}", "  User:".cyan(), info.user);
    if let Some(ns) = &info.namespace {
        println!("{} {}", "  Namespace:".cyan(), ns);
    } else {
        println!("{} {}", "  Namespace:".cyan(), "(default)".dimmed());
    }

    Ok(())
}

/// Interactive switch with fzf.
/// Opens fzf immediately, then loads contexts from kubeconfig in a background
/// thread (context loading is local file I/O so this is near-instant).
fn op_interactive_switch() -> Result<(), String> {
    let kc =
        kubeconfig::Kubeconfig::load_default().map_err(|e| format!("kubeconfig error: {}", e))?;
    let current = kc.get_current_context();

    let selected = fzf::fuzzy_select_streaming(current.as_deref(), || {
        let kc = kubeconfig::Kubeconfig::load_default().unwrap();
        kc.get_contexts()
    })
    .ok_or_else(|| "no context selected".to_string())?;
    op_switch(
        &selected,
        &GlobalFlags {
            dry_run: false,
            output_json: false,
            show_health: false,
        },
    )
}

// Simple JSON serialization for --output json (avoid adding serde_json dependency)
mod serde_json {
    pub struct Entry {
        pub name: String,
        pub current: bool,
    }

    pub fn to_string(entries: &[Entry]) -> String {
        let mut items = Vec::new();
        for e in entries {
            items.push(format!(
                r#"{{"name":"{}","current":{}}}"#,
                escape(&e.name),
                e.current
            ));
        }
        format!("[{}]", items.join(","))
    }

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }
}
