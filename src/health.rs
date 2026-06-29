// Cluster health checking.
#![allow(dead_code)]
//
// Pings each context's API server to check if it's reachable.
// Uses `timeout kubectl --context=<name> get namespace default`
// to make a real API call with a hard process timeout.
// Results are collected concurrently via threads.

use std::process::{Command, Stdio};
use std::time::Duration;

/// Health status for a single context.
#[derive(Clone, Debug)]
pub enum Health {
    /// Cluster is reachable.
    Healthy,
    /// Cluster is unreachable or timed out.
    Unreachable,
    /// Health check was not performed (disabled or skipped).
    Unknown,
}

impl Health {
    /// Returns a colored indicator string for terminal display.
    /// Uses different symbols so health is visible even without color:
    ///   ● green = healthy, ✗ red = unreachable
    pub fn indicator(&self) -> String {
        use colored::Colorize;
        match self {
            Health::Healthy => "●".green().to_string(),
            Health::Unreachable => "✗".red().to_string(),
            Health::Unknown => " ".to_string(),
        }
    }
}

/// Check the health of a single context.
/// Uses `timeout` to wrap kubectl so the process is hard-killed after
/// a few seconds. `kubectl get namespace default` makes a real API call
/// that actually connects to the server (unlike `cluster-info` which
/// just reads the local config).
pub fn check_context_health(context: &str) -> Health {
    // Try using the `timeout` command (GNU coreutils on macOS, built-in on Linux)
    let result = Command::new("timeout")
        .arg("4")
        .arg("kubectl")
        .arg("--context")
        .arg(context)
        .arg("get")
        .arg("namespace")
        .arg("default")
        .arg("--request-timeout=3s")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                Health::Healthy
            } else {
                Health::Unreachable
            }
        }
        Err(_) => {
            // `timeout` not found — fall back to manual polling with kill
            check_context_health_fallback(context)
        }
    }
}

/// Fallback health check without the `timeout` command.
/// Spawns kubectl directly and polls with try_wait, killing after 5 seconds.
fn check_context_health_fallback(context: &str) -> Health {
    let mut child = match Command::new("kubectl")
        .arg("--context")
        .arg(context)
        .arg("get")
        .arg("namespace")
        .arg("default")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Health::Unreachable,
    };

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Health::Healthy
                } else {
                    Health::Unreachable
                };
            }
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(5) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Health::Unreachable;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Health::Unreachable,
        }
    }
}

/// Check health of multiple contexts concurrently.
/// Returns a map of context name → Health.
pub fn check_all_health(contexts: &[String]) -> std::collections::HashMap<String, Health> {
    let mut handles = Vec::new();

    for ctx in contexts {
        let ctx = ctx.clone();
        let handle = std::thread::spawn(move || (ctx.clone(), check_context_health(&ctx)));
        handles.push(handle);
    }

    let mut results = std::collections::HashMap::new();
    for handle in handles {
        if let Ok((ctx, health)) = handle.join() {
            results.insert(ctx, health);
        }
    }
    results
}