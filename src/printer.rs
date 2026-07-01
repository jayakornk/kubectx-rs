// Colored output helpers.

#![allow(dead_code)]

use std::io::{self, IsTerminal, Write};

use colored::*;

/// Check if stdout is a terminal (for interactive mode detection).
pub fn is_interactive() -> bool {
    io::stdout().is_terminal()
}

/// Print the list of contexts, highlighting the current one.
pub fn print_context_list(contexts: &[String], current: Option<&str>) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for ctx in contexts {
        if Some(ctx.as_str()) == current {
            let _ = writeln!(out, "{} {}", "→".green().bold(), ctx.cyan().bold());
        } else {
            let _ = writeln!(out, "  {}", ctx);
        }
    }
}

/// Print the list of contexts with health indicators.
pub fn print_context_list_with_health(
    contexts: &[String],
    current: Option<&str>,
    health: &std::collections::HashMap<String, crate::health::Health>,
) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for ctx in contexts {
        let marker = if Some(ctx.as_str()) == current {
            "→".green().bold().to_string()
        } else {
            " ".to_string()
        };
        let health_indicator = health
            .get(ctx)
            .map(|h| h.indicator())
            .unwrap_or_else(|| " ".to_string());
        let name = if Some(ctx.as_str()) == current {
            ctx.cyan().bold().to_string()
        } else {
            ctx.clone()
        };
        let _ = writeln!(out, "{} {} {}", marker, health_indicator, name);
    }
}

/// Print the list of namespaces, highlighting the current one.
pub fn print_namespace_list(namespaces: &[String], current: Option<&str>) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for ns in namespaces {
        if Some(ns.as_str()) == current {
            let _ = writeln!(out, "{} {}", "→".green().bold(), ns.cyan().bold());
        } else {
            let _ = writeln!(out, "  {}", ns);
        }
    }
}

/// Print "Switched to context" message.
pub fn print_switched_context(ctx: &str) {
    println!(
        "{} \"{}\"",
        "Switched to context".green().bold(),
        ctx.cyan()
    );
}

/// Print "Switched to namespace" message.
pub fn print_switched_namespace(ns: &str) {
    println!(
        "{} \"{}\"",
        "Switched to namespace".green().bold(),
        ns.cyan()
    );
}

/// Print an error message.
pub fn print_error(msg: &str) {
    eprintln!("{} {}", "error:".red().bold(), msg);
}

/// Print a warning/info message.
pub fn print_info(msg: &str) {
    eprintln!("{}", msg.yellow());
}

/// Print a success message.
pub fn print_success(msg: &str) {
    println!("{}", msg.green());
}
