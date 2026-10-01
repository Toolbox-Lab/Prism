#[cfg(feature = "decode")]
pub mod archive;
#[cfg(feature = "decode")]
pub mod cache;
#[cfg(feature = "decode")]
pub mod debugger;
#[cfg(feature = "decode")]
pub mod decode;
pub mod error;
#[cfg(feature = "decode")]
pub mod network;
#[cfg(feature = "decode")]
pub mod replay;
#[cfg(feature = "decode")]
pub mod rpc;
#[cfg(feature = "decode")]
pub mod spec;
pub mod taxonomy;
#[cfg(feature = "decode")]
pub mod types;
pub mod xdr;

#[cfg(feature = "decode")]
pub use decode::{
    scval_to_json, scval_to_json_with_contract_spec, scval_to_json_with_spec,
    walk_diagnostic_events, AddressCredential, AddressWithNonce, ArgumentDecoder, AuthChain,
    AuthCredential, AuthFunctionKind, AuthInvocation, DecodedArgument, DecodedFunctionCall,
    DiagnosticEventKind, DiagnosticEventWalker, EnumDecoder, FunctionCallDecoder, MultiOpDecoder,
    ReturnValueDecoder, StructDecodeReport, StructDecoder, StructuredDiagnosticEvent,
};
pub use error::{GratError, GratResult};
#[cfg(feature = "decode")]
pub use network::config::Network;
#[cfg(feature = "decode")]
pub use types::address::Address;
#[cfg(feature = "decode")]
pub use types::config::NetworkConfig;
#[cfg(feature = "decode")]
pub use types::report::DiagnosticReport;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SOROBAN_PROTOCOL_VERSION: u32 =
    soroban_env_host::meta::get_ledger_protocol_version(soroban_env_host::meta::INTERFACE_VERSION);

#[cfg(test)]
#[ctor::ctor]
fn init_test_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("grat_core=debug,soroban_env_host=warn"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}
