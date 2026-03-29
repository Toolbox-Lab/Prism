#![cfg(test)]

use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str;

fn init_git_repo(path: &Path) {
    let output = Command::new("git")
        .current_dir(path)
        .arg("init")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("config")
        .arg("user.email")
        .arg("whatever@example.com")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("config")
        .arg("user.name")
        .arg("whatever")
        .output()
        .unwrap();
    assert!(output.status.success());

    let file = path.join("readme");
    fs::write(file, "hello").unwrap();

    let output = Command::new("git")
        .current_dir(path)
        .arg("add")
        .arg("readme")
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new("git")
        .current_dir(path)
        .arg("commit")
        .arg("-am")
        .arg("test")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn test_init() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]+";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_init_subdir() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let manifest_dir = git_dir.join("subdir");
    std::fs::create_dir(&manifest_dir).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, &manifest_dir);
    assert!(res.is_ok());

    // In the subdir case, `git rev-parse --git-dir` returns an absolute path
    // with symlinks resolved. This shows up on macs where the tempdir includes
    // a symlink to /private/var.
    let git_dir = std::fs::canonicalize(git_dir).unwrap();

    let out = str::from_utf8(&out).unwrap();
    let expected = &format!(
        "cargo:rerun-if-changed={gd}/.git/index
cargo:rerun-if-changed={gd}/.git/HEAD
cargo:rerun-if-changed={gd}/.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]+",
        gd = git_dir.display()
    );
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_dirty() {
    let tempdir = tempfile::tempdir().unwrap();
    let git_dir = tempdir.path();

    init_git_repo(git_dir);

    let file = git_dir.join("readme");
    fs::write(file, "dirty").unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, git_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rerun-if-changed=.git/index
cargo:rerun-if-changed=.git/HEAD
cargo:rerun-if-changed=.git/refs
cargo:rustc-env=GIT_REVISION=[0-9a-f]+-dirty";
    println!("{out}");
    println!("{expected}");
    assert!(Regex::new(expected).unwrap().is_match(out));
}

#[test]
fn test_published() {
    let tempdir = tempfile::tempdir().unwrap();
    let crate_dir = tempdir.path();

    let vcs_info = r#"{
  "git": {
    "sha1": "0c5255b6f47649305fcb68edccb285510aec71a7"
  },
  "path_in_vcs": ""
}"#;

    let file = crate_dir.join(".cargo_vcs_info.json");
    fs::write(file, vcs_info).unwrap();

    let mut out = Vec::new();
    let res = super::__init(&mut out, crate_dir);
    assert!(res.is_ok());
    let out = str::from_utf8(&out).unwrap();
    let expected = "cargo:rustc-env=GIT_REVISION=0c5255b6f47649305fcb68edccb285510aec71a7\n";
    println!("{out}");
    println!("{expected}");
    assert_eq!(out, expected);
}
