use std::sync::atomic::{AtomicBool, Ordering.}

pub mod disk;
pub mod provider;
pub mod store;
pub mod wasm;

static BYPASS_CACHE: AtomicBool = AtomicBool::new(false);

pub fn set_bypass(enabled: bool) {
    BYPASS_CACHE.store(enabled, Ordering::Relaxed);
    store::set_bypass(enabled);
}

pub fn is_bypass_enabled() -> bool {
    BYPASS_CACHE.load(Ordering::Relaxed)
}
