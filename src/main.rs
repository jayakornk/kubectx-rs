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
//   kubectx -h, --help            : show this message
//   kubectx -V, --version         : show version

mod kubeconfig;
mod printer;
mod fzf;
mod state;

use std::env;
use std::process::ExitCode;

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
                                   (this command won't delete the user/cluster entry
                                    referenced by the context entry)
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

fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        // No args: list contexts or interactive fzf mode
        if printer::is_interactive() && fzf::fzf_available() {
            return op_interactive_switch();
        }
        return op_list();
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
            return op_unset();
        }
        "-d" | "--delete" => {
            return op_delete(&args[1..]);
        }
        _ => {}
    }

    // "-" → swap to previous context
    if arg == "-" {
        return op_swap();
    }

    // "<NEW>=<OLD>" → rename
    if let Some(eq_pos) = arg.find('=') {
        let new_name = &arg[..eq_pos];
        let old_name = &arg[eq_pos + 1..];
        return op_rename(new_name, old_name);
    }

    // "<NAME>" → switch to context
    op_switch(arg)
}

/// List all available contexts.
fn op_list() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let contexts = kc.get_contexts();
    let current = kc.get_current_context();
    if contexts.is_empty() {
        return Err("no contexts found in kubeconfig".into());
    }
    printer::print_context_list(&contexts, current.as_deref());
    Ok(())
}

/// Show the current context name.
fn op_current() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    match kc.get_current_context() {
        Some(ctx) => {
            println!("{}", ctx);
            Ok(())
        }
        None => Err("current-context is not set".into()),
    }
}

/// Switch to a context by name.
fn op_switch(name: &str) -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

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

    kc.set_current_context(name)
        .map_err(|e| format!("failed to set current context: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_switched_context(name);
    Ok(())
}

/// Swap to the previous context.
fn op_swap() -> Result<(), String> {
    let prev_file = state::prev_context_file()
        .ok_or_else(|| "failed to determine state file path".to_string())?;
    let prev = state::read_state(&prev_file)
        .map_err(|e| format!("failed to read previous context file: {}", e))?;
    if prev.is_empty() {
        return Err("no previous context found".into());
    }
    op_switch(&prev)
}

/// Unset the current context.
fn op_unset() -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    kc.unset_current_context()
        .map_err(|e| format!("failed to unset current context: {}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success("Context unset");
    Ok(())
}

/// Rename a context.
fn op_rename(new_name: &str, old_name: &str) -> Result<(), String> {
    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    kc.rename_context(new_name, old_name)
        .map_err(|e| format!("{}", e))?;
    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    printer::print_success(&format!(
        "Context \"{}\" renamed to \"{}\".",
        old_name, new_name
    ));
    Ok(())
}

/// Delete one or more contexts.
fn op_delete(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        // Interactive deletion with fzf
        if printer::is_interactive() && fzf::fzf_available() {
            return op_delete_interactive();
        }
        return Err("specify at least one context to delete".into());
    }

    let mut kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;

    for name in names {
        let target = if name == "." {
            kc.get_current_context()
                .ok_or_else(|| "no current context to delete".to_string())?
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

    kc.save().map_err(|e| format!("failed to save kubeconfig: {}", e))?;
    Ok(())
}

/// Interactive deletion with fzf.
fn op_delete_interactive() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let contexts = kc.get_contexts();
    let current = kc.get_current_context();
    if contexts.is_empty() {
        return Err("no contexts found in kubeconfig".into());
    }
    let selected = fzf::fuzzy_select(&contexts, current.as_deref())
        .ok_or_else(|| "no context selected".to_string())?;
    op_delete(&[selected])
}

/// Interactive switch with fzf.
fn op_interactive_switch() -> Result<(), String> {
    let kc = kubeconfig::Kubeconfig::load_default()
        .map_err(|e| format!("kubeconfig error: {}", e))?;
    let contexts = kc.get_contexts();
    let current = kc.get_current_context();
    if contexts.is_empty() {
        return Err("no contexts found in kubeconfig".into());
    }
    let selected = fzf::fuzzy_select(&contexts, current.as_deref())
        .ok_or_else(|| "no context selected".to_string())?;
    op_switch(&selected)
}