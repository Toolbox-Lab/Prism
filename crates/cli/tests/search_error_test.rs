use assert_cmd::Command;
use serde_json::Value;

fn grat() -> Command {
    Command::cargo_bin("grat").expect(
        "Failed to find or build 'grat' binary. Ensure the project compiles on this platform.",
    )
}

#[test]
fn test_search_error_json_output_is_a_valid_json_array() {
    let mut cmd = grat();
    cmd.args(["search-error", "budget", "--output", "json", "--offline"]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("search-error --output json must emit valid JSON ({e}):\n{stdout}")
    });

    let entries = payload
        .as_array()
        .unwrap_or_else(|| panic!("search-error --output json must emit a JSON array:\n{stdout}"));

    assert!(
        !entries.is_empty(),
        "expected at least one taxonomy entry matching 'budget':\n{stdout}"
    );

    for entry in entries {
        let object = entry
            .as_object()
            .unwrap_or_else(|| panic!("expected JSON objects in the results array:\n{stdout}"));

        for field in ["category", "code", "name", "summary", "severity"] {
            assert!(
                object.contains_key(field),
                "result entry is missing the '{field}' field:\n{stdout}"
            );
        }

        assert!(object["category"].is_string());
        assert!(object["code"].is_u64());
        assert!(object["name"].is_string());
        assert!(object["summary"].is_string());
        assert!(object["severity"].is_string());
    }
}

#[test]
fn test_search_error_json_output_is_an_empty_array_when_nothing_matches() {
    let mut cmd = grat();
    cmd.args([
        "search-error",
        "zzz-no-such-taxonomy-entry-zzz",
        "--output",
        "json",
        "--offline",
    ]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("search-error --output json must emit valid JSON ({e}):\n{stdout}")
    });

    let entries = payload
        .as_array()
        .unwrap_or_else(|| panic!("search-error --output json must emit a JSON array:\n{stdout}"));

    assert!(
        entries.is_empty(),
        "expected an empty JSON array when no entry matches:\n{stdout}"
    );
}

#[test]
fn test_search_error_finds_entries_by_common_cause() {
    // Regression test for the documented example: "balance" only appears in the
    // `common_causes` remediation text of the embedded taxonomy.
    let mut cmd = grat();
    cmd.args(["search-error", "balance", "--offline"]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(
        stdout.contains("ContractError") && stdout.contains("IntegerDivisionByZero"),
        "expected entries whose common causes mention 'balance':\n{stdout}"
    );
}

#[test]
fn test_search_error_human_output_still_renders_a_table() {
    let mut cmd = grat();
    cmd.args(["search-error", "budget", "--offline"]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(
        stdout.contains("Category") && stdout.contains("Code") && stdout.contains("Summary"),
        "human output should still render the table headers:\n{stdout}"
    );
    assert!(
        serde_json::from_str::<Value>(&stdout).is_err(),
        "human output should not be JSON:\n{stdout}"
    );
}
