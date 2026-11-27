use assert_cmd::Command;

fn grat() -> Command {
    Command::cargo_bin("grat").expect("grat binary should build")
}

#[test]
fn profile_is_listed_as_a_top_level_command() {
    let assert = grat().arg("--help").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(stdout.contains("profile"), "help output:\n{stdout}");
}

#[test]
fn profile_help_documents_its_contract() {
    let assert = grat().args(["profile", "--help"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(
        stdout.contains("<TX_HASH>"),
        "profile help output:\n{stdout}"
    );
    assert!(
        stdout.contains("Transaction hash to replay and profile"),
        "profile help output:\n{stdout}"
    );
    assert!(
        stdout.contains("--output"),
        "profile help output:\n{stdout}"
    );
    assert!(stdout.contains("--save"), "profile help output:\n{stdout}");
}

#[test]
fn profile_without_a_transaction_hash_fails_with_usage_error() {
    let assert = grat().arg("profile").assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);

    assert!(
        stderr.contains("TX_HASH"),
        "expected clap usage error mentioning TX_HASH, got:\n{stderr}"
    );
}
