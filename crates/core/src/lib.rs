pub mod archive;
pub mod cache;
pub mod debugger;
pub mod decode;
pub mod error;
pub mod network;
pub mod replay;
pub mod rpc;
pub mod spec;
pub mod taxonomy;
pub mod types;
pub mod xdr;

pub use decode::{
    walk_diagnostic_events, AddressCredential, AuthChain, AuthCredential, AuthFunctionKind,
    AuthInvocation, DiagnosticEventKind, DiagnosticEventWalker, StructuredDiagnosticEvent,
};
pub use error::{PrismError, PrismResult};
pub use network::config::Network;
pub use types::address::Address;
pub use types::config::NetworkConfig;
pub use types::report::DiagnosticReport;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SOROBAN_PROTOCOL_VERSION: u32 =
    soroban_env_host::meta::get_ledger_protocol_version(soroban_env_host::meta::INTERFACE_VERSION);

#[cfg(test)]
#[ctor::ctor]
fn init_test_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("prism_core=debug,soroban_env_host=warn"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}
