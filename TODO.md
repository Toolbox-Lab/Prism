# TODO - CacheStore LRU Eviction Fix

- [x] Implement directory-aware size calculation in `crates/core/src/cache/store.rs`
- [x] Add LRU eviction triggered by `put()` when `current_size + new_entry_size > max_size`
- [x] Scan cache files, sort by last access timestamp (fallback to modified), delete oldest until enough space
- [x] Update access semantics on `get()` so reads affect LRU ordering
- [x] Add/extend unit tests validating aggregate size stays within limit after evictions
- [x] Run `cargo test` to verify correctness
