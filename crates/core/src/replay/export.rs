//! Turns a reconstructed transaction ([`LedgerState`]) into a standalone Rust
//! integration test that replays the exact host-function invocation through
//! `soroban-env-host`, so an on-chain failure can be reproduced (and kept as a
//! regression test) without any network access.
//!
//! The generated file depends on `soroban-env-host` only: every XDR payload
//! (host function, resources, source account, auth entries, footprint ledger
//! entries, TTL entries, cost parameters) is embedded as a byte-array literal
//! and decoded with the host's own re-exported XDR types.

use crate::error::{GratError, GratResult};
use crate::replay::sandbox::{encode_ttl_entry, MEM_BYTES_CEILING};
use crate::replay::state::{encode_xdr, LedgerState};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Version requirement written into the generated file's header. Kept in step
/// with the `soroban-env-host` this crate replays with.
pub const SOROBAN_ENV_HOST_REQUIREMENT: &str = "21.2";

/// How the transaction ended on-chain; the generated test asserts the local
/// replay ends the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedOutcome {
    Success,
    Failure,
}

impl ExpectedOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

/// Everything needed to render one exported test case.
#[derive(Debug, Clone)]
pub struct TestCaseSpec<'a> {
    pub tx_hash: &'a str,
    pub network: &'a str,
    pub expected: ExpectedOutcome,
    pub state: &'a LedgerState,
}

/// Summary of a rendered test, suitable for `--output json`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportedTestCase {
    pub tx_hash: String,
    pub network: String,
    pub ledger_sequence: u32,
    pub expected_outcome: ExpectedOutcome,
    pub test_name: String,
    pub file_name: String,
    pub ledger_entries: usize,
    pub auth_entries: usize,
    pub source: String,
}

/// Lower-cased, hex-only form of a transaction hash, or an error when the
/// input is not a 64-character hex string.
pub fn normalize_tx_hash(tx_hash: &str) -> GratResult<String> {
    let trimmed = tx_hash.trim();
    if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GratError::ReplayError(format!(
            "invalid transaction hash {trimmed:?}: expected 64 hex characters"
        )));
    }
    Ok(trimmed.to_ascii_lowercase())
}

/// Default file name for an exported test: `grat_test_<hash>.rs`.
pub fn default_file_name(tx_hash: &str) -> String {
    format!("grat_test_{}.rs", tx_hash.to_ascii_lowercase())
}

/// Name of the `#[test]` function inside the generated file.
pub fn test_function_name(tx_hash: &str) -> String {
    let short: String = tx_hash.chars().take(16).collect();
    format!("reproduce_tx_{}", short.to_ascii_lowercase())
}

/// Renders the standalone integration test for `spec`.
pub fn render_test_case(spec: &TestCaseSpec<'_>) -> GratResult<ExportedTestCase> {
    let state = spec.state;
    let inv = &state.invocation;
    let tx_hash = spec.tx_hash.to_ascii_lowercase();

    // Same entry/TTL ordering the sandbox uses: footprint order, read-only
    // first, skipping keys with no ledger entry yet.
    let mut ledger_entries: Vec<&[u8]> = Vec::new();
    let mut ttl_entries: Vec<Vec<u8>> = Vec::new();
    for key_b64 in inv.read_only_keys.iter().chain(inv.read_write_keys.iter()) {
        let Some(entry) = state.entries.get(key_b64) else {
            continue;
        };
        ledger_entries.push(entry);
        ttl_entries.push(match inv.ttl_by_key.get(key_b64) {
            Some(live_until) => encode_ttl_entry(key_b64, *live_until)?,
            None => Vec::new(),
        });
    }

    let cpu_params = encode_xdr(&inv.cpu_cost_params, "ContractCostParams")?;
    let mem_params = encode_xdr(&inv.mem_cost_params, "ContractCostParams")?;
    let seed: [u8; 32] = Sha256::digest(tx_hash.as_bytes()).into();
    let test_name = test_function_name(&tx_hash);
    let expect_success = spec.expected == ExpectedOutcome::Success;

    let mut out = String::new();
    let w = &mut out;
    // `write!` into a String cannot fail.
    let _ = writeln!(w, "//! Regression test exported by `grat export`.");
    let _ = writeln!(w, "//!");
    let _ = writeln!(w, "//! Transaction: {tx_hash}");
    let _ = writeln!(w, "//! Network:     {}", spec.network);
    let _ = writeln!(w, "//! Ledger:      {}", state.ledger_sequence);
    let _ = writeln!(w, "//! On-chain:    {}", spec.expected.as_str());
    let _ = writeln!(w, "//!");
    let _ = writeln!(
        w,
        "//! Replays the transaction's host function against the footprint ledger"
    );
    let _ = writeln!(
        w,
        "//! entries captured at export time, entirely offline. Drop this file into a"
    );
    let _ = writeln!(w, "//! crate's `tests/` directory with:");
    let _ = writeln!(w, "//!");
    let _ = writeln!(w, "//!     [dev-dependencies]");
    let _ = writeln!(
        w,
        "//!     soroban-env-host = \"{SOROBAN_ENV_HOST_REQUIREMENT}\""
    );
    let _ = writeln!(w, "//!");
    let _ = writeln!(w, "//! then run `cargo test {test_name} -- --nocapture`.");
    let _ = writeln!(w);
    let _ = writeln!(w, "#![allow(clippy::all, clippy::pedantic)]");
    let _ = writeln!(w);
    let _ = writeln!(w, "use soroban_env_host::budget::Budget;");
    let _ = writeln!(w, "use soroban_env_host::e2e_invoke::invoke_host_function;");
    let _ = writeln!(
        w,
        "use soroban_env_host::xdr::{{ContractCostParams, Limits, ReadXdr}};"
    );
    let _ = writeln!(w, "use soroban_env_host::LedgerInfo;");
    let _ = writeln!(w);
    let _ = writeln!(w, "const TX_HASH: &str = \"{tx_hash}\";");
    let _ = writeln!(w, "const EXPECT_SUCCESS: bool = {expect_success};");
    let _ = writeln!(w, "const LEDGER_SEQUENCE: u32 = {};", state.ledger_sequence);
    let _ = writeln!(w, "const PROTOCOL_VERSION: u32 = {};", inv.protocol_version);
    let _ = writeln!(w, "const TIMESTAMP: u64 = {};", inv.timestamp);
    let _ = writeln!(w, "const BASE_RESERVE: u32 = {};", inv.base_reserve);
    let _ = writeln!(
        w,
        "const MIN_TEMP_ENTRY_TTL: u32 = {};",
        inv.min_temp_entry_ttl
    );
    let _ = writeln!(
        w,
        "const MIN_PERSISTENT_ENTRY_TTL: u32 = {};",
        inv.min_persistent_entry_ttl
    );
    let _ = writeln!(w, "const MAX_ENTRY_TTL: u32 = {};", inv.max_entry_ttl);
    let _ = writeln!(
        w,
        "const CPU_INSTRUCTIONS_LIMIT: u64 = {};",
        inv.cpu_instructions_limit.max(1)
    );
    let _ = writeln!(w, "const MEM_BYTES_LIMIT: u64 = {MEM_BYTES_CEILING};");
    let _ = writeln!(w);
    write_bytes_const(w, "NETWORK_ID", "[u8; 32]", &inv.network_id);
    write_bytes_const(w, "PRNG_SEED", "[u8; 32]", &seed);
    write_bytes_const(w, "HOST_FUNCTION", "&[u8]", &inv.host_function_xdr);
    write_bytes_const(w, "RESOURCES", "&[u8]", &inv.resources_xdr);
    write_bytes_const(w, "SOURCE_ACCOUNT", "&[u8]", &inv.source_account_xdr);
    write_bytes_const(w, "CPU_COST_PARAMS", "&[u8]", &cpu_params);
    write_bytes_const(w, "MEM_COST_PARAMS", "&[u8]", &mem_params);
    write_bytes_list(
        w,
        "AUTH_ENTRIES",
        inv.auth_entries_xdr.iter().map(Vec::as_slice),
    );
    write_bytes_list(w, "LEDGER_ENTRIES", ledger_entries.iter().copied());
    write_bytes_list(w, "TTL_ENTRIES", ttl_entries.iter().map(Vec::as_slice));

    out.push_str(&TEST_BODY.replace("__TEST_NAME__", &test_name));

    Ok(ExportedTestCase {
        tx_hash: tx_hash.clone(),
        network: spec.network.to_string(),
        ledger_sequence: state.ledger_sequence,
        expected_outcome: spec.expected,
        test_name,
        file_name: default_file_name(&tx_hash),
        ledger_entries: ledger_entries.len(),
        auth_entries: inv.auth_entries_xdr.len(),
        source: out,
    })
}

const TEST_BODY: &str = r#"fn owned(list: &[&[u8]]) -> Vec<Vec<u8>> {
    list.iter().map(|b| b.to_vec()).collect()
}

#[test]
fn __TEST_NAME__() {
    let cpu_params = ContractCostParams::from_xdr(CPU_COST_PARAMS, Limits::none())
        .expect("decode CPU cost params");
    let mem_params = ContractCostParams::from_xdr(MEM_COST_PARAMS, Limits::none())
        .expect("decode memory cost params");
    let budget = Budget::try_from_configs(
        CPU_INSTRUCTIONS_LIMIT,
        MEM_BYTES_LIMIT,
        cpu_params,
        mem_params,
    )
    .expect("build budget");

    let ledger_info = LedgerInfo {
        protocol_version: PROTOCOL_VERSION,
        sequence_number: LEDGER_SEQUENCE,
        timestamp: TIMESTAMP,
        network_id: NETWORK_ID,
        base_reserve: BASE_RESERVE,
        min_temp_entry_ttl: MIN_TEMP_ENTRY_TTL,
        min_persistent_entry_ttl: MIN_PERSISTENT_ENTRY_TTL,
        max_entry_ttl: MAX_ENTRY_TTL,
    };

    let mut diagnostic_events = Vec::new();
    let result = invoke_host_function(
        &budget,
        true,
        HOST_FUNCTION.to_vec(),
        RESOURCES.to_vec(),
        SOURCE_ACCOUNT.to_vec(),
        owned(AUTH_ENTRIES).into_iter(),
        ledger_info,
        owned(LEDGER_ENTRIES).into_iter(),
        owned(TTL_ENTRIES).into_iter(),
        PRNG_SEED.to_vec(),
        &mut diagnostic_events,
    )
    .expect("host setup failed before the invocation ran");

    for (i, event) in diagnostic_events.iter().enumerate() {
        println!("diagnostic[{i}]: {event:?}");
    }
    match &result.encoded_invoke_result {
        Ok(_) => println!("{TX_HASH}: invocation succeeded"),
        Err(e) => println!("{TX_HASH}: invocation failed: {e:?}"),
    }

    assert_eq!(
        result.encoded_invoke_result.is_ok(),
        EXPECT_SUCCESS,
        "replay of {TX_HASH} did not reproduce the on-chain outcome"
    );
}
"#;

const BYTES_PER_LINE: usize = 24;

fn write_bytes(out: &mut String, bytes: &[u8], indent: &str) {
    if bytes.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for chunk in bytes.chunks(BYTES_PER_LINE) {
        out.push_str(indent);
        out.push_str("    ");
        let line: Vec<String> = chunk.iter().map(u8::to_string).collect();
        out.push_str(&line.join(", "));
        out.push_str(",\n");
    }
    out.push_str(indent);
    out.push(']');
}

fn write_bytes_const(out: &mut String, name: &str, ty: &str, bytes: &[u8]) {
    let _ = write!(out, "#[rustfmt::skip]\nconst {name}: {ty} = ");
    if ty.starts_with('&') {
        out.push('&');
    }
    write_bytes(out, bytes, "");
    out.push_str(";\n\n");
}

fn write_bytes_list<'b>(out: &mut String, name: &str, items: impl Iterator<Item = &'b [u8]>) {
    let _ = write!(out, "#[rustfmt::skip]\nconst {name}: &[&[u8]] = &[");
    let mut any = false;
    for item in items {
        if !any {
            out.push('\n');
            any = true;
        }
        out.push_str("    &");
        write_bytes(out, item, "    ");
        out.push_str(",\n");
    }
    out.push_str("];\n\n");
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::replay::state::{InvocationInput, ReconstructionPath};
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::collections::HashMap;
    use stellar_xdr::curr::{
        AccountId, ContractCostParamEntry, ContractCostParams, ContractCostType,
        ContractDataDurability, ExtensionPoint, Hash, HostFunction, InvokeContractArgs,
        LedgerFootprint, LedgerKey, LedgerKeyContractData, PublicKey, ScAddress, ScSymbol, ScVal,
        SorobanResources, Uint256,
    };

    /// Golden file: the test this module renders for [`missing_instance_state`].
    /// It is also compiled and run as an integration test of this crate, which
    /// proves the generated code builds and reproduces the failure offline.
    /// Regenerate with `GRAT_UPDATE_GOLDEN=1 cargo test -p grat-core export`.
    const GOLDEN_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/exported_missing_instance.rs"
    );

    const GOLDEN_HASH: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";

    fn zero_cost_params() -> ContractCostParams {
        let entries: Vec<ContractCostParamEntry> = ContractCostType::VARIANTS
            .iter()
            .map(|_| ContractCostParamEntry {
                ext: ExtensionPoint::V0,
                const_term: 0,
                linear_term: 0,
            })
            .collect();
        ContractCostParams(entries.try_into().unwrap())
    }

    fn key_b64(key: &LedgerKey) -> String {
        STANDARD.encode(encode_xdr(key, "LedgerKey").unwrap())
    }

    /// A failed invocation: `hello()` on a contract whose instance entry is in
    /// the footprint but absent from the ledger.
    pub(crate) fn missing_instance_state() -> LedgerState {
        let contract = ScAddress::Contract(Hash([7; 32]));
        let instance_key = LedgerKey::ContractData(LedgerKeyContractData {
            contract: contract.clone(),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
        });
        let host_fn = HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: contract,
            function_name: ScSymbol("hello".try_into().unwrap()),
            args: Vec::new().try_into().unwrap(),
        });
        let resources = SorobanResources {
            footprint: LedgerFootprint {
                read_only: vec![instance_key.clone()].try_into().unwrap(),
                read_write: Vec::new().try_into().unwrap(),
            },
            instructions: 10_000_000,
            read_bytes: 10_000,
            write_bytes: 0,
        };
        let source = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32])));
        let network_id: [u8; 32] = Sha256::digest(b"Test SDF Network ; September 2015").into();

        LedgerState {
            ledger_sequence: 1_000,
            entries: HashMap::new(),
            reconstruction_path: ReconstructionPath::HotPath,
            invocation: InvocationInput {
                host_function_xdr: encode_xdr(&host_fn, "HostFunction").unwrap(),
                resources_xdr: encode_xdr(&resources, "SorobanResources").unwrap(),
                source_account_xdr: encode_xdr(&source, "AccountId").unwrap(),
                auth_entries_xdr: Vec::new(),
                read_only_keys: vec![key_b64(&instance_key)],
                read_write_keys: Vec::new(),
                ttl_by_key: HashMap::new(),
                cpu_cost_params: zero_cost_params(),
                mem_cost_params: zero_cost_params(),
                network_id,
                protocol_version: crate::SOROBAN_PROTOCOL_VERSION,
                timestamp: 1_700_000_000,
                base_reserve: 5_000_000,
                min_temp_entry_ttl: 16,
                min_persistent_entry_ttl: 4_096,
                max_entry_ttl: 3_110_400,
                cpu_instructions_limit: 10_000_000,
            },
        }
    }

    #[test]
    fn rendered_test_matches_golden_file() {
        let state = missing_instance_state();
        let exported = render_test_case(&TestCaseSpec {
            tx_hash: GOLDEN_HASH,
            network: "testnet",
            expected: ExpectedOutcome::Failure,
            state: &state,
        })
        .unwrap();

        assert_eq!(exported.file_name, format!("grat_test_{GOLDEN_HASH}.rs"));
        assert_eq!(exported.test_name, "reproduce_tx_c0ffee00c0ffee00");
        assert_eq!(exported.ledger_entries, 0);
        assert!(exported
            .source
            .contains("const EXPECT_SUCCESS: bool = false;"));
        assert!(exported
            .source
            .contains("fn reproduce_tx_c0ffee00c0ffee00()"));

        if std::env::var_os("GRAT_UPDATE_GOLDEN").is_some() {
            std::fs::write(GOLDEN_PATH, &exported.source).unwrap();
        }
        let golden = std::fs::read_to_string(GOLDEN_PATH).unwrap_or_default();
        assert_eq!(
            exported.source, golden,
            "exported test drifted from {GOLDEN_PATH}; rerun with GRAT_UPDATE_GOLDEN=1"
        );
    }

    #[test]
    fn ledger_entries_follow_footprint_order_and_carry_ttls() {
        let mut state = missing_instance_state();
        let ro = state.invocation.read_only_keys[0].clone();
        state.entries.insert(ro.clone(), vec![1, 2, 3]);
        state.invocation.ttl_by_key.insert(ro.clone(), 5_000);
        state.invocation.read_write_keys.push("absent".to_string());

        let exported = render_test_case(&TestCaseSpec {
            tx_hash: GOLDEN_HASH,
            network: "testnet",
            expected: ExpectedOutcome::Success,
            state: &state,
        })
        .unwrap();
        assert_eq!(exported.ledger_entries, 1);
        assert!(exported
            .source
            .contains("const EXPECT_SUCCESS: bool = true;"));
        let ttl = encode_ttl_entry(&ro, 5_000).unwrap();
        let ttl_literal: Vec<String> = ttl.iter().take(4).map(u8::to_string).collect();
        assert!(exported.source.contains(&ttl_literal.join(", ")));
        assert!(exported
            .source
            .contains("const LEDGER_ENTRIES: &[&[u8]] = &[\n    &[\n        1, 2, 3,\n    ],\n];"));
    }

    #[test]
    fn normalize_tx_hash_accepts_hex_and_lowercases() {
        let h = "AB".repeat(32);
        assert_eq!(normalize_tx_hash(&h).unwrap(), "ab".repeat(32));
        assert!(normalize_tx_hash("abc").is_err());
        assert!(normalize_tx_hash(&"zz".repeat(32)).is_err());
    }

    #[test]
    fn names_derive_from_hash() {
        let h = "0123456789abcdef".repeat(4);
        assert_eq!(default_file_name(&h), format!("grat_test_{h}.rs"));
        assert_eq!(test_function_name(&h), "reproduce_tx_0123456789abcdef");
    }

    #[test]
    fn byte_literals_round_trip_shape() {
        let mut s = String::new();
        write_bytes_const(&mut s, "X", "&[u8]", &[1, 2, 255]);
        assert_eq!(
            s,
            "#[rustfmt::skip]\nconst X: &[u8] = &[\n    1, 2, 255,\n];\n\n"
        );
        let mut s = String::new();
        write_bytes_list(&mut s, "L", std::iter::empty());
        assert_eq!(s, "#[rustfmt::skip]\nconst L: &[&[u8]] = &[];\n\n");
    }
}
