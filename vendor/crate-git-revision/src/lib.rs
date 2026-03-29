//! Embed the git revision of a crate in its build.
//!
//! Supports embedding the version from a local or remote git repository the build
//! is occurring in, as well as when `cargo install` or depending on a crate
//! published to crates.io.
//!
//! It extracts the git revision in two ways:
//! - From the `.cargo_vcs_info.json` file embedded in published crates.
//! - From the git repository the build is occurring from in unpublished crates.
//!
//! Injects an environment variable `GIT_REVISION` into the build that contains
//! the full git revision, with a `-dirty` suffix if the working directory is
//! dirty.
//!
//! Requires the use of a build.rs build script. See [Build Scripts]() for more
//! details on how Rust build scripts work.
//!
//! [Build Scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html
//!
//! ### Examples
//!
//! Add the following to the crate's `Cargo.toml` file:
//!
//! ```toml
//! [build_dependencies]
//! crate-git-revision = "0.0.2"
//! ```
//!
//! Add the following to the crate's `build.rs` file:
//!
//! ```rust
//! crate_git_revision::init();
//! ```
//!
//! Add the following to the crate's `lib.rs` or `main.rs` file:
//!
//! ```ignore
//! pub const GIT_REVISION: &str = env!("GIT_REVISION");
//! ```

use std::{fs::read_to_string, path::Path, process::Command, str};

/// Initialize the GIT_REVISION environment variable with the git revision of
/// the current crate.
///
/// Intended to be called from within a build script, `build.rs` file, for the
/// crate.
pub fn init() {
    let _res = __init(&mut std::io::stdout(), &std::env::current_dir().unwrap());
}

fn __init(w: &mut impl std::io::Write, current_dir: &Path) -> std::io::Result<()> {
    let mut git_sha: Option<String> = None;

    // Read the git revision from the JSON file embedded by cargo publish. This
    // will get the version from published crates.
    if let Ok(vcs_info) = read_to_string(current_dir.join(".cargo_vcs_info.json")) {
        let vcs_info: Result<CargoVcsInfo, _> = serde_json::from_str(&vcs_info);
        if let Ok(vcs_info) = vcs_info {
            git_sha = Some(vcs_info.git.sha1);
        }
    }

    // Read the git revision from the git repository containing the code being
    // built.
    if git_sha.is_none() {
        match Command::new("git")
            .current_dir(current_dir)
            .arg("rev-parse")
            .arg("--git-dir")
            .output()
            .map(|o| o.stdout)
        {
            Err(e) => {
                writeln!(
                    w,
                    "cargo:warning=Error getting git directory to get git revision: {e:?}"
                )?;
            }
            Ok(git_dir) => {
                let git_dir = String::from_utf8_lossy(&git_dir);
                let git_dir = git_dir.trim();

                // Require the build script to rerun if relavent git state changes which
                // changes the current git commit.
                //  - .git/index: Changes if the index/staged files changes, which will
                //  cause the repo to be dirty.
                //  - .git/HEAD: Changes if the ref currently in the working directory,
                //  and potentially the commit, to change.
                //  - .git/refs: Changes to any files in refs could cause the current
                //  commit to have changed if the ref in .git/HEAD is changed.
                // Note: That changes in the above files may not result in material
                // changes to the crate, but changes in any should invalidate the
                // revision since the revision can be changed by any of the above.
                writeln!(w, "cargo:rerun-if-changed={git_dir}/index")?;
                writeln!(w, "cargo:rerun-if-changed={git_dir}/HEAD")?;
                writeln!(w, "cargo:rerun-if-changed={git_dir}/refs")?;

                match Command::new("git")
                    .current_dir(current_dir)
                    .arg("describe")
                    .arg("--always")
                    .arg("--exclude='*'")
                    .arg("--long")
                    .arg("--abbrev=1000")
                    .arg("--dirty")
                    .output()
                    .map(|o| o.stdout)
                {
                    Err(e) => {
                        writeln!(
                            w,
                            "cargo:warning=Error getting git revision from {current_dir:?}: {e:?}"
                        )?;
                    }
                    Ok(git_describe) => {
                        git_sha = str::from_utf8(&git_describe).ok().map(str::to_string);
                    }
                }
            }
        }
    }

    if let Some(git_sha) = git_sha {
        writeln!(w, "cargo:rustc-env=GIT_REVISION={git_sha}")?;
    }

    Ok(())
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default)]
struct CargoVcsInfo {
    git: CargoVcsInfoGit,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default)]
struct CargoVcsInfoGit {
    sha1: String,
}

mod test;
