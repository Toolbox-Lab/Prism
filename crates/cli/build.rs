use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-env-changed=GRAT_BUILD_HASH");

    let build_hash = std::env::var("GRAT_BUILD_HASH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(git_hash);

    println!("cargo:rustc-env=GRAT_BUILD_HASH={build_hash}");
}

fn git_hash() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output();

    match output {
        Ok(output) if output.status.success() => String::from_utf8(output.stdout)
            .map_or_else(|_| "unknown".to_owned(), |hash| hash.trim().to_owned()),
        _ => "unknown".to_owned(),
    }
}
