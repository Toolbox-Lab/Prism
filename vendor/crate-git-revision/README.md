# crate-git-revision

Embed the git revision of a crate in its build.

Supports embedding the version from a local or remote git repository the build
is occurring in, as well as when `cargo install` or depending on a crate
published to crates.io.

It extracts the git revision in two ways:
- From the `.cargo_vcs_info.json` file embedded in published crates.
- From the git repository the build is occurring from in unpublished crates.

Injects an environment variable `GIT_REVISION` into the build that contains
the full git revision, with a `-dirty` suffix if the working directory is
dirty.

Requires the use of a build.rs build script. See [Build Scripts]() for more
details on how Rust build scripts work.

[Build Scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html

#### Examples

Add the following to the crate's `Cargo.toml` file:

```toml
[build_dependencies]
crate-git-revision = "0.0.2"
```

Add the following to the crate's `build.rs` file:

```rust
crate_git_revision::init();
```

Add the following to the crate's `lib.rs` or `main.rs` file:

```rust
pub const GIT_REVISION: &str = env!("GIT_REVISION");
```

License: Apache-2.0
