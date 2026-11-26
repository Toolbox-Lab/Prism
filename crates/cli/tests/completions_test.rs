use assert_cmd::Command;

fn grat() -> Command {
    Command::cargo_bin("grat").expect(
        "Failed to find or build 'grat' binary. Ensure the project compiles on this platform.",
    )
}

fn completions(shell: &str) -> String {
    let assert = grat().args(["completions", shell]).assert().success();
    let output = assert.get_output();

    assert!(
        output.stderr.is_empty(),
        "completions {shell} must not write to stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout.clone()).expect("completion script should be valid UTF-8")
}

#[test]
fn test_completions_emit_a_script_for_every_supported_shell() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let script = completions(shell);
        assert!(
            script.contains("grat"),
            "{shell} completion script should reference the grat binary:\n{script}"
        );
    }
}

#[test]
fn test_bash_completions_define_the_completion_function() {
    let script = completions("bash");
    assert!(
        script.contains("_grat()"),
        "missing _grat() function:\n{script}"
    );
    assert!(
        script.contains("complete -F _grat"),
        "missing `complete -F _grat` registration:\n{script}"
    );
}

#[test]
fn test_zsh_completions_start_with_compdef_header() {
    let script = completions("zsh");
    assert!(
        script.starts_with("#compdef grat"),
        "zsh script should start with `#compdef grat`:\n{script}"
    );
}

#[test]
fn test_completions_list_subcommands_and_global_flags() {
    let script = completions("bash");

    for subcommand in [
        "decode",
        "inspect",
        "trace",
        "profile",
        "diff",
        "replay",
        "whatif",
        "export",
        "db",
        "clean",
        "auth",
        "diagnostic",
        "history",
        "completions",
        "serve",
        "search-error",
    ] {
        assert!(
            script.contains(subcommand),
            "missing subcommand `{subcommand}`"
        );
    }

    for flag in [
        "--output",
        "--network",
        "--verbose",
        "--rpc-url",
        "--save",
        "--config-path",
        "--quiet",
        "--no-color",
        "--offline",
        "--no-cache",
    ] {
        assert!(script.contains(flag), "missing global flag `{flag}`");
    }
}

#[test]
fn test_completions_reject_unknown_shell() {
    grat().args(["completions", "tcsh"]).assert().failure();
}
