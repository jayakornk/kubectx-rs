# kubectx-rs

A Rust port of [kubectx](https://github.com/ahmetb/kubectx) — a faster way to switch between clusters and namespaces in kubectl.

## Features

### kubectx
- **List contexts** — `kubectx` shows all available contexts, highlighting the current one
- **Switch context** — `kubectx <NAME>` switches to the named context
- **Switch to previous** — `kubectx -` toggles back to the previous context
- **Show current** — `kubectx -c` / `--current` prints the current context name
- **Unset current** — `kubectx -u` / `--unset` clears the current-context
- **Rename context** — `kubectx <NEW>=<OLD>` renames a context; `kubectx <NEW>=.` renames the current context
- **Delete context** — `kubectx -d <NAME> [<NAME>...]` deletes one or more contexts (`.` for current)
- **Interactive mode** — when `fzf` is installed and stdout is a TTY, `kubectx` launches a fuzzy-search menu
- **Help & version** — `kubectx -h` / `--help` and `kubectx -V` / `--version`

### kubens
- **List namespaces** — `kubens` shows namespaces in the current context
- **Switch namespace** — `kubens <NAME>` sets the namespace for the current context
- **Switch to previous** — `kubens -` toggles to the previous namespace
- **Show/unset/delete/rename** — same flags as kubectx, applied to namespaces

## Building

```shell
cargo build --release
```

Binaries are at `target/release/kubectx` and `target/release/kubens`.

## Installation

Copy the binaries to your `PATH`:

```shell
cp target/release/kubectx target/release/kubens /usr/local/bin/
```

## Usage

```
USAGE:
  kubectx                       : list the contexts
  kubectx <NAME>                : switch to context <NAME>
  kubectx -                     : switch to the previous context
  kubectx -c, --current         : show the current context name
  kubectx -u, --unset           : unset the current context
  kubectx <NEW_NAME>=<NAME>     : rename context <NAME> to <NEW_NAME>
  kubectx <NEW_NAME>=.          : rename current-context to <NEW_NAME>
  kubectx -d <NAME> [<NAME...>] : delete context <NAME> ('.' for current-context)
  kubectx -h, --help            : show this message
  kubectx -V, --version         : show version
```

## State files

- Previous context: `~/.kube/kubectx/prev_context`
- Previous namespace: `~/.kube/kubens/prev_namespace`

## KUBECONFIG support

- Respects the `KUBECONFIG` environment variable (colon-separated on Unix, semicolon on Windows)
- Falls back to `~/.kube/config` when `KUBECONFIG` is not set
- Multiple files are merged for reading; the file containing the entry is modified for writing

## Interactive mode (fzf)

If [`fzf`](https://github.com/junegunn/fzf) is installed and stdout is a terminal, `kubectx` and `kubens` with no arguments launch an interactive fuzzy-search menu. Set `KUBECTX_IGNORE_FZF=1` to disable.

## Testing

```shell
cargo test              # unit tests
bash tests/integration_test.sh   # integration tests
```

## Differences from the Go original

- Written in Rust for zero-dependency static binaries (no Go runtime)
- Does not include the `--shell` / `--readonly` sub-shell features (planned for future)
- Namespace listing shells out to `kubectl get namespaces` instead of using the Kubernetes client-go library
- Uses `serde_yaml` for YAML parsing instead of `sigs.k8s.io/yaml`

## License

Apache-2.0 (same as the original kubectx)