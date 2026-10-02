//! Taxonomy Linter — validate taxonomy TOML files against the schema.
//!
//! Usage:
//!   taxonomy_linter [<directory>]
//!
//! Defaults to `crates/core/src/taxonomy/data` when no argument is given.
//! Exits with status 0 if no issues found, 1 otherwise.

use std::path::PathBuf;
use std::process;

fn main() {
    let dir: PathBuf = std::env::args_os().nth(1).map_or_else(
        || {
            // Default to the data directory relative to the binary's workspace root.
            let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            p.push("src/taxonomy/data");
            p
        },
        PathBuf::from,
    );

    if !dir.is_dir() {
        eprintln!("error: '{}' is not a directory", dir.display());
        process::exit(1);
    }

    match grat_core::taxonomy::linter::lint_dir(&dir) {
        Ok(()) => {
            println!("✅ No issues found in {}", dir.display());
            process::exit(0);
        }
        Err(e) => {
            eprintln!("error: lint_dir execution failed: {e}");
            process::exit(1);
        }
    }
}
