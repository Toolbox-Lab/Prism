**Prism --quiet flag implementation**

- [x] 1. Edit `crates/cli/src/main.rs`: Added global --quiet arg, dispatch calls (fixed).

- [x] 2. Edit main.rs dispatch calls to pass &cli.quiet.

- [x] 3. Edit `crates/cli/src/commands/decode.rs`: Add quiet param, wrap spinner.

- [x] 4. Edit `crates/cli/src/commands/trace.rs`: Add param, wrap spinner & file msg.

- [x] 4. Edit `crates/cli/src/commands/trace.rs`: Add param, wrap spinner & file msg.

- [x] 5. Edit `crates/cli/src/commands/diff.rs`: Add param, wrap spinner & header.

- [x] 6. Edit `crates/cli/src/commands/inspect.rs`: Add param, wrap spinner.

- [x] 7. Edit `crates/cli/src/commands/profile.rs`: Add param, wrap spinner & header.

- [x] 8. Edit `crates/cli/src/commands/replay.rs`: Add param, wrap prints.

- [x] 9. Edit `crates/cli/src/commands/whatif.rs`: Add param, wrap prints.

- [x] 10. Edit `crates/cli/src/commands/export.rs`: Add param, wrap prints.

- [x] 11. Edit `crates/cli/src/commands/db.rs`: Add param, wrap prints.

- [x] 12. `cd crates/cli && cargo check` (passed).

- [ ] 13. Test `prism decode FAKE --quiet`
