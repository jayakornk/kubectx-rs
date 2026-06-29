// Shell completion script generation.
#![allow(dead_code)]
//
// Generates completion scripts for bash, zsh, and fish that auto-complete
// context names (kubectx) and namespace names (kubens).
//
// The completion scripts call the binary itself with a hidden `__complete`
// subcommand to get the list of context/namespace names dynamically.

/// Generate a completion script for the given shell.
///
/// `binary` is "kubectx" or "kubens".
/// `shell` is "bash", "zsh", or "fish".
pub fn generate(binary: &str, shell: &str) -> String {
    match shell {
        "bash" => generate_bash(binary),
        "zsh" => generate_zsh(binary),
        "fish" => generate_fish(binary),
        _ => format!(
            "error: unsupported shell '{}'. Use: bash, zsh, or fish",
            shell
        ),
    }
}

/// List supported shells.
pub fn supported_shells() -> &'static [&'static str] {
    &["bash", "zsh", "fish"]
}

fn generate_bash(binary: &str) -> String {
    format!(
        r#"# bash completion for {bin}
_{bin}_complete() {{
    local cur prev
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"

    # Complete context/namespace names for the first positional argument
    if [ "$COMP_CWORD" = "1" ]; then
        local items
        items=$({bin} __complete 2>/dev/null)
        if [ -n "$items" ]; then
            COMPREPLY=($(compgen -W "$items" -- "$cur"))
        fi
    fi

    # Complete flags
    if [[ "$cur" == -* ]]; then
        local flags="-h --help -V --version -c --current -u --unset -d -s --shell -r --readonly -i --info --dry-run -o --output --health --aliases"
        COMPREPLY=($(compgen -W "$flags" -- "$cur"))
    fi

    return 0
}}
complete -F _{bin}_complete {bin}
"#,
        bin = binary
    )
}

fn generate_zsh(binary: &str) -> String {
    format!(
        r#"#compdef {bin}
# zsh completion for {bin}

_{bin}() {{
    local -a items flags
    local curcontext="$curcontext" state line

    _arguments -C \
        '-h[show help]' \
        '--help[show help]' \
        '-V[show version]' \
        '--version[show version]' \
        '-c[show current context/namespace]' \
        '--current[show current context/namespace]' \
        '-u[unset current context/namespace]' \
        '--unset[unset current context/namespace]' \
        '-d[delete]' \
        '-s[start isolated shell]:context:' \
        '--shell[start isolated shell]:context:' \
        '-r[start read-only shell]:context:' \
        '--readonly[start read-only shell]:context:' \
        '-i[show info]:context:' \
        '--info[show info]:context:' \
        '--dry-run[dry run mode]' \
        '-o[output format]:format:(json text)' \
        '--output[output format]:format:(json text)' \
        '--health[show health indicators]' \
        '--aliases[list aliases]' \
        '1: :->items' \

    case $state in
        items)
            local list
            list=($({bin} __complete 2>/dev/null))
            _describe '{bin}' list
            ;;
    esac
}}

_{bin} "$@"
"#,
        bin = binary
    )
}

fn generate_fish(binary: &str) -> String {
    format!(
        r#"# fish completion for {bin}

# Complete context/namespace names
complete -c {bin} -f -a '({bin} __complete)'

# Complete flags
complete -c {bin} -f -n '__fish_use_subcommand' -s h -l help -d 'show help'
complete -c {bin} -f -n '__fish_use_subcommand' -s V -l version -d 'show version'
complete -c {bin} -f -n '__fish_use_subcommand' -s c -l current -d 'show current context/namespace'
complete -c {bin} -f -n '__fish_use_subcommand' -s u -l unset -d 'unset current context/namespace'
complete -c {bin} -f -n '__fish_use_subcommand' -s d -d 'delete context/namespace'
complete -c {bin} -f -n '__fish_use_subcommand' -s s -l shell -d 'start isolated shell'
complete -c {bin} -f -n '__fish_use_subcommand' -s r -l readonly -d 'start read-only shell'
complete -c {bin} -f -n '__fish_use_subcommand' -s i -l info -d 'show context/namespace info'
complete -c {bin} -f -n '__fish_use_subcommand' -l dry-run -d 'dry run mode'
complete -c {bin} -f -n '__fish_use_subcommand' -s o -l output -d 'output format' -a 'json text'
complete -c {bin} -f -n '__fish_use_subcommand' -l health -d 'show health indicators'
complete -c {bin} -f -n '__fish_use_subcommand' -l aliases -d 'list aliases'
"#,
        bin = binary
    )
}