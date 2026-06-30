# kubectx-rs

A Rust port of **[kubectx](https://github.com/ahmetb/kubectx)** by [Ahmet Alp Balkan](https://github.com/ahmetb) — a faster way to switch between clusters and namespaces in kubectl. All credit for the original design, features, and idea goes to the upstream project.

> [!WARNING]
> **AI-Generated — Use at Your Own Risk**
>
> The entire codebase in this repository was written and ported by an AI. No human has reviewed, audited, or verified any of the code. There are no guarantees of correctness, security, or fitness for any purpose. **You are solely responsible for reviewing and validating this software before using it in any environment — especially production.**

## Features

### kubectx
- **List contexts** — `kubectx` shows all available contexts, highlighting the current one
- **Switch context** — `kubectx <NAME>` switches to the named context
- **Switch to previous** — `kubectx -` toggles back to the previous context
- **Show current** — `kubectx -c` / `--current` prints the current context name
- **Unset current** — `kubectx -u` / `--unset` clears the current-context
- **Rename context** — `kubectx <NEW>=<OLD>` renames a context; `kubectx <NEW>=.` renames the current context
- **Delete context** — `kubectx -d <NAME> [<NAME>...]` deletes one or more contexts (`.` for current)
- **Context info** — `kubectx -i <NAME>` / `--info` shows cluster, server, user, and namespace details (`.` for current)
- **Health indicators** — `kubectx --health` pings each cluster's API server and shows green/red dots
- **Dry-run** — `kubectx --dry-run <NAME>` shows what would change without modifying the kubeconfig
- **JSON output** — `kubectx -o json` / `--output json` for machine-readable output
- **Isolated shell** — `kubectx -s <NAME>` / `--shell` starts a shell scoped to a single context
- **Read-only shell** — `kubectx -r <NAME>` / `--readonly` starts a shell that blocks write operations
- **Context aliases** — `kubectx @prod=gke_long_name` sets an alias; `kubectx @prod` switches by alias; `kubectx --aliases` lists all
- **Interactive mode** — when `fzf` is installed and stdout is a TTY, `kubectx` launches a fullscreen fuzzy-search menu with background loading
- **Shell completions** — `kubectx completion bash|zsh|fish` generates completion scripts
- **Help & version** — `kubectx -h` / `--help` and `kubectx -V` / `--version`

### kubens
- **List namespaces** — `kubens` queries the live cluster via `kubectl get namespaces` for the full list
- **Switch namespace** — `kubens <NAME>` sets the namespace for the current context
- **Force switch** — `kubens -f <NAME>` / `--force` switches even if the namespace doesn't exist in the cluster
- **Switch to previous** — `kubens -` toggles to the previous namespace
- **Show/unset/delete/rename** — same flags as kubectx, applied to namespaces
- **Dry-run** — `kubens --dry-run` shows what would change
- **JSON output** — `kubens -o json` for machine-readable output
- **Shell completions** — `kubens completion bash|zsh|fish`
- **Interactive fzf** — fullscreen fuzzy-search with background cluster querying

## Building

```shell
cargo build --release
```

Binaries are at `target/release/kubectx` and `target/release/kubens`.

## Installation

> [!IMPORTANT]
> **This port ships the same `kubectx` and `kubens` command names as the original.** Installing both will conflict. If you already have the original `kubectx` (Go) installed via `brew install kubectx` or otherwise, **uninstall it first** before installing this Rust port — otherwise the two binaries will shadow each other and you may unknowingly run the wrong one.

### Homebrew (via tap)

```shell
# If the original kubectx is installed, remove it first
brew uninstall kubectx          # removes the Go original (ahmetb/kubectx)

# Add the tap and install
brew tap jayakornk/tap
brew install kubectx-rs          # installs both kubectx and kubens

# Upgrading later
brew upgrade kubectx-rs
```

### Build from source

```shell
# If the original kubectx is installed, remove it first
brew uninstall kubectx          # if installed via Homebrew

# Clone and build

git clone https://github.com/jayakornk/kubectx-rs.git
cd kubectx-rs
cargo build --release

# Install via cargo
cargo install --path .

# Or copy the binaries directly
sudo cp target/release/kubectx target/release/kubens /usr/local/bin/
# On Apple Silicon Homebrew:
# sudo cp target/release/kubectx target/release/kubens /opt/homebrew/bin/
```

## Shell completions

```shell
# zsh (with oh-my-zsh)
mkdir -p ~/.oh-my-zsh/custom/completions
kubectx completion zsh > ~/.oh-my-zsh/custom/completions/_kubectx
kubens completion zsh > ~/.oh-my-zsh/custom/completions/_kubens

# zsh (with zinit or plain zsh)
# Install to a directory in your $fpath (e.g. /opt/homebrew/share/zsh/site-functions)
kubectx completion zsh > /opt/homebrew/share/zsh/site-functions/_kubectx
kubens completion zsh > /opt/homebrew/share/zsh/site-functions/_kubens
# Or use ~/.zsh/functions if that's in your $fpath
# kubectx completion zsh > ~/.zsh/functions/_kubectx
# kubens completion zsh > ~/.zsh/functions/_kubens
# Then reload: exec zsh

# bash
kubectx completion bash > /etc/bash_completion.d/kubectx
kubens completion bash > /etc/bash_completion.d/kubens

# fish
kubectx completion fish > ~/.config/fish/completions/kubectx.fish
kubens completion fish > ~/.config/fish/completions/kubens.fish
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
  kubectx -V, --version         : show version
```

## State files

- Previous context: `~/.kube/kubectx/prev_context`
- Previous namespace: `~/.kube/kubens/prev_namespace`
- Context aliases: `~/.kube/kubectx/aliases`

Automatically migrates from the old format where `~/.kube/kubectx` was a file.

## KUBECONFIG support

- Respects the `KUBECONFIG` environment variable (colon-separated on Unix, semicolon on Windows)
- Falls back to `~/.kube/config` when `KUBECONFIG` is not set
- Deduplicates file paths (handles repeated entries in `KUBECONFIG`)
- Multiple files are merged for reading; the file containing the entry is modified for writing

## Interactive mode (fzf)

If [`fzf`](https://github.com/junegunn/fzf) is installed and stdout is a terminal, `kubectx` and `kubens` with no arguments launch a fullscreen fuzzy-search menu. Items are loaded in a background thread and stream into fzf as they arrive — the user can start searching immediately. Set `KUBECTX_IGNORE_FZF=1` to disable.

## Testing

```shell
cargo test              # unit tests
bash tests/integration_test.sh   # integration tests
```

## Releasing

Releases are automated. The only manual step is bumping the version and tagging, which [`cargo-release`](https://github.com/crate-ci/cargo-release) handles for you.

### One-command release

```shell
cargo release patch -x      # 0.1.0 -> 0.1.1   (also: minor, major)
```

`cargo-release` is configured in `[package.metadata.release]` in `Cargo.toml` and will:

- bump `version` in `Cargo.toml` and `Cargo.lock`,
- commit it as `release: <version>`,
- create and push a `v<version>` tag.

> `cargo-release` is dry-run by default; the `-x` / `--execute` flag applies the changes.
>
> The version-bump commit (`chore(release): <version>`) follows Conventional Commits, so git-cliff excludes it from the changelog — only `feat`/`fix`/`perf`/etc. appear.

### What then happens automatically (on the pushed `v*` tag)

The `release` GitHub Actions workflow runs, gated so nothing ships unless every check passes:

1. **verify-version** — fails fast if `Cargo.toml`'s version doesn't match the tag, so a mismatched tag can never produce a broken release.
2. **test** — `cargo fmt --check` + `cargo test --locked`.
3. **security** — `cargo audit --deny warnings` (RustSec advisory scan of `Cargo.lock`).
4. **build** — precompiles `kubectx` + `kubens` for macOS (arm64/x86_64) and Linux (x86_64/arm64).
5. **release** — generates a changelog from Conventional Commits via [git-cliff](https://github.com/orhun/git-cliff) (`cliff.toml`) and publishes a GitHub Release with the changelog, binaries, and checksums.
6. **tap-bump** — updates the `kubectx-rs` formula in [`jayakornk/homebrew-tap`](https://github.com/jayakornk/homebrew-tap) (new `url` + `sha256`) so `brew upgrade kubectx-rs` works immediately.

`ci.yml` independently runs the same fmt/test/audit checks on every push to `main` and on pull requests.

### Manual release (without cargo-release)

```shell
# 1. bump the version to match the tag you intend to create
$EDITOR Cargo.toml           # version = "0.2.0"
cargo build -q                # refresh Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "release: 0.2.0"
# 2. tag that exact commit and push
git tag v0.2.0
git push origin main --tags
```

The `verify-version` guard enforces that `Cargo.toml`'s version equals the tag; if they differ the release aborts before building anything.

## Acknowledgements

This project is a port of [**kubectx**](https://github.com/ahmetb/kubectx) by [Ahmet Alp Balkan](https://github.com/ahmetb). All credit for the original tool, its feature design, and its UX belongs to the upstream project and its contributors.

## Differences from the Go original

- Written in Rust for fast, self-contained binaries
- Namespace listing shells out to `kubectl get namespaces` instead of using client-go
- Read-only shell uses a kubectl wrapper script instead of an HTTP proxy
- Uses `serde_yaml_ng` for YAML parsing instead of `sigs.k8s.io/yaml`
- Added: `--info`, `--health`, `--dry-run`, `--output json`, context aliases

## License

Apache-2.0 (same as the original kubectx)