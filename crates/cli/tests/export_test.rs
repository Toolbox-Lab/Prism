use assert_cmd::Command;

fn grat() -> Command {
    Command::cargo_bin("grat").expect(
        "Failed to find or build 'grat' binary. Ensure the project compiles on this platform.",
    )
}

#[test]
fn export_help_lists_its_options() {
    let assert = grat().args(["export", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    for flag in ["--format", "--out", "<TX_HASH>"] {
        assert!(
            stdout.contains(flag),
            "export --help is missing {flag}:\n{stdout}"
        );
    }
}

#[test]
fn export_out_flag_does_not_clash_with_global_output() {
    // `--output json` (global) and `--out <path>` (export) must parse together.
    grat()
        .args(["export", "not-a-hash", "--output", "json", "--out", "x.rs"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid transaction hash"));
}

#[test]
fn export_rejects_unknown_format() {
    grat()
        .args(["export", &"ab".repeat(32), "--format", "trace"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid value 'trace'"));
}
