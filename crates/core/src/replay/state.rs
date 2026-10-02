use crate::error::{GratError, GratResult};
use crate::types::config::NetworkConfig;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use stellar_xdr::curr::{
    AccountId, ConfigSettingEntry, ConfigSettingId, ContractCostParams, FeeBumpTransactionInnerTx,
    InvokeHostFunctionOp, LedgerEntry, LedgerEntryData, LedgerKey, LedgerKeyConfigSetting, Limits,
    MuxedAccount, OperationBody, PublicKey, ReadXdr, StateArchivalSettings, Transaction,
    TransactionEnvelope, TransactionExt, TransactionV1Envelope, WriteXdr,
};

/// Everything the sandbox needs, beyond raw ledger entries, to actually invoke
/// the transaction's host function through `soroban-env-host`.
///
/// All XDR payloads are pre-encoded (rather than kept as typed values) because
/// that's the shape `soroban_env_host::e2e_invoke::invoke_host_function_with_trace_hook`
/// consumes directly — encoding here means the sandbox never has to re-derive it.
#[derive(Debug, Clone)]
pub struct InvocationInput {
    pub host_function_xdr: Vec<u8>,
    pub resources_xdr: Vec<u8>,
    pub source_account_xdr: Vec<u8>,
    pub auth_entries_xdr: Vec<Vec<u8>>,

    /// base64(LedgerKey XDR) for every key in the transaction's declared footprint.
    pub read_only_keys: Vec<String>,
    pub read_write_keys: Vec<String>,

    /// base64(LedgerKey XDR) -> live-until ledger, for footprint keys that carry a TTL.
    pub ttl_by_key: HashMap<String, u32>,

    pub cpu_cost_params: ContractCostParams,
    pub mem_cost_params: ContractCostParams,

    pub network_id: [u8; 32],
    pub protocol_version: u32,
    pub timestamp: u64,
    pub base_reserve: u32,
    pub min_temp_entry_ttl: u32,
    pub min_persistent_entry_ttl: u32,
    pub max_entry_ttl: u32,

    /// The instruction budget the transaction itself was allowed to consume at
    /// consensus time; reused as the replay CPU ceiling so a successful mainnet
    /// transaction never gets cut short here.
    pub cpu_instructions_limit: u64,
}

#[derive(Debug, Clone)]
pub struct LedgerState {
    pub ledger_sequence: u32,

    /// base64(LedgerKey XDR) -> raw LedgerEntry XDR bytes, for every footprint
    /// key that currently exists in the ledger.
    pub entries: HashMap<String, Vec<u8>>,

    pub reconstruction_path: ReconstructionPath,

    pub invocation: InvocationInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructionPath {
    HotPath,

    ColdPath,
}

const HOT_PATH_THRESHOLD: u32 = 50_000;

/// Stellar's network base reserve (in stroops) has been unchanged since genesis
/// on every public network; it isn't exposed via a soroban-rpc ledger-entry
/// lookup, so it's a documented constant rather than a network round-trip.
const BASE_RESERVE_STROOPS: u32 = 5_000_000;

pub async fn reconstruct_state(tx_hash: &str, network: &NetworkConfig) -> GratResult<LedgerState> {
    let rpc = crate::rpc::SorobanRpcClient::new(network);

    let tx_data = rpc.get_transaction(tx_hash).await?;
    let tx_ledger = tx_data
        .ledger
        .ok_or_else(|| GratError::ReplayError("Cannot determine transaction ledger".to_string()))?;

    let latest: serde_json::Value = rpc.get_latest_ledger().await?;
    let latest_ledger = latest
        .get("sequence")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let protocol_version = parse_protocol_version(latest.get("protocolVersion"));

    let age = latest_ledger.saturating_sub(tx_ledger);

    if age <= HOT_PATH_THRESHOLD {
        tracing::info!("Using hot path (RPC) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_hot_path(tx_ledger, protocol_version, &tx_data, &rpc, network).await
    } else {
        tracing::info!("Using cold path (archive) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_cold_path(tx_ledger, network).await
    }
}

fn parse_protocol_version(value: Option<&serde_json::Value>) -> u32 {
    value
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        })
        .unwrap_or(0) as u32
}

#[allow(clippy::too_many_lines)]
async fn reconstruct_hot_path(
    ledger_sequence: u32,
    protocol_version: u32,
    tx_data: &crate::rpc::client::GetTransactionResponse,
    rpc: &crate::rpc::SorobanRpcClient,
    network: &NetworkConfig,
) -> GratResult<LedgerState> {
    let envelope_xdr = tx_data.envelope_xdr.as_deref().ok_or_else(|| {
        GratError::ReplayError("Transaction envelope XDR is missing from getTransaction".into())
    })?;
    let envelope_bytes =
        STANDARD
            .decode(envelope_xdr)
            .map_err(|e| GratError::XdrDecodingFailed {
                type_name: "TransactionEnvelope",
                reason: format!("invalid base64: {e}"),
            })?;
    let envelope = TransactionEnvelope::from_xdr(&envelope_bytes, Limits::none()).map_err(|e| {
        GratError::XdrDecodingFailed {
            type_name: "TransactionEnvelope",
            reason: e.to_string(),
        }
    })?;

    let tx = extract_transaction(&envelope)?;
    let invoke_op = extract_invoke_host_function_op(tx)?;

    let soroban_data = match &tx.ext {
        TransactionExt::V1(data) => data,
        TransactionExt::V0 => {
            return Err(GratError::ReplayError(
                "Transaction has no Soroban resource data attached".to_string(),
            ))
        }
    };

    let read_only_keys = encode_keys(soroban_data.resources.footprint.read_only.iter())?;
    let read_write_keys = encode_keys(soroban_data.resources.footprint.read_write.iter())?;

    let cpu_cost_key = config_setting_key(ConfigSettingId::ContractCostParamsCpuInstructions);
    let mem_cost_key = config_setting_key(ConfigSettingId::ContractCostParamsMemoryBytes);
    let archival_key = config_setting_key(ConfigSettingId::StateArchival);
    let cpu_cost_key_b64 = encode_key(&cpu_cost_key)?;
    let mem_cost_key_b64 = encode_key(&mem_cost_key)?;
    let archival_key_b64 = encode_key(&archival_key)?;

    let mut all_keys = read_only_keys.clone();
    all_keys.extend(read_write_keys.iter().cloned());
    all_keys.push(cpu_cost_key_b64.clone());
    all_keys.push(mem_cost_key_b64.clone());
    all_keys.push(archival_key_b64.clone());

    let response = rpc.get_ledger_entries(&all_keys).await?;
    let entries_json = response
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            GratError::ReplayError("getLedgerEntries response is missing an entries array".into())
        })?;

    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    let mut ttl_by_key: HashMap<String, u32> = HashMap::new();
    let mut cpu_cost_params: Option<ContractCostParams> = None;
    let mut mem_cost_params: Option<ContractCostParams> = None;
    let mut archival_settings: Option<StateArchivalSettings> = None;

    for entry_json in entries_json {
        let key_b64 = entry_json.get("key").and_then(serde_json::Value::as_str);
        let xdr_b64 = entry_json
            .get("xdr")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                GratError::ReplayError("getLedgerEntries entry is missing its xdr field".into())
            })?;
        let entry_bytes = STANDARD
            .decode(xdr_b64)
            .map_err(|e| GratError::XdrDecodingFailed {
                type_name: "LedgerEntry",
                reason: format!("invalid base64: {e}"),
            })?;

        let key_b64 = key_b64.ok_or_else(|| {
            GratError::ReplayError(
                "getLedgerEntries entry is missing its key field — RPC node is too old to \
                 support replay"
                    .to_string(),
            )
        })?;

        let ledger_entry = LedgerEntry::from_xdr(&entry_bytes, Limits::none()).map_err(|e| {
            GratError::XdrDecodingFailed {
                type_name: "LedgerEntry",
                reason: e.to_string(),
            }
        })?;

        if key_b64 == cpu_cost_key_b64 {
            if let LedgerEntryData::ConfigSetting(
                ConfigSettingEntry::ContractCostParamsCpuInstructions(p),
            ) = ledger_entry.data
            {
                cpu_cost_params = Some(p);
            }
        } else if key_b64 == mem_cost_key_b64 {
            if let LedgerEntryData::ConfigSetting(
                ConfigSettingEntry::ContractCostParamsMemoryBytes(p),
            ) = ledger_entry.data
            {
                mem_cost_params = Some(p);
            }
        } else if key_b64 == archival_key_b64 {
            if let LedgerEntryData::ConfigSetting(ConfigSettingEntry::StateArchival(s)) =
                ledger_entry.data
            {
                archival_settings = Some(s);
            }
        } else {
            if let Some(live_until) = entry_json
                .get("liveUntilLedgerSeq")
                .and_then(serde_json::Value::as_u64)
            {
                ttl_by_key.insert(key_b64.to_string(), live_until as u32);
            }
            entries.insert(key_b64.to_string(), entry_bytes);
        }
    }

    let cpu_cost_params = cpu_cost_params.ok_or_else(|| {
        GratError::ReplayError(
            "Network CPU cost parameters are unavailable — cannot build an execution budget"
                .to_string(),
        )
    })?;
    let mem_cost_params = mem_cost_params.ok_or_else(|| {
        GratError::ReplayError(
            "Network memory cost parameters are unavailable — cannot build an execution budget"
                .to_string(),
        )
    })?;
    let archival_settings = archival_settings.ok_or_else(|| {
        GratError::ReplayError(
            "Network state-archival settings are unavailable — cannot determine entry TTLs"
                .to_string(),
        )
    })?;

    let source_account = muxed_account_to_account_id(&tx.source_account);
    let host_function_xdr = encode_xdr(&invoke_op.host_function, "HostFunction")?;
    let resources_xdr = encode_xdr(&soroban_data.resources, "SorobanResources")?;
    let source_account_xdr = encode_xdr(&source_account, "AccountId")?;
    let auth_entries_xdr = invoke_op
        .auth
        .iter()
        .map(|a| encode_xdr(a, "SorobanAuthorizationEntry"))
        .collect::<GratResult<Vec<_>>>()?;

    let network_id: [u8; 32] = Sha256::digest(network.network_passphrase.as_bytes()).into();

    // The exact close time of the historical ledger isn't exposed by
    // getTransaction; the latest ledger's close time is used as a documented
    // approximation (only observable difference for time-sensitive contract
    // logic within the hot-path recency window).
    let timestamp = tx_data.latest_ledger_close_time.unwrap_or(0);

    Ok(LedgerState {
        ledger_sequence,
        entries,
        reconstruction_path: ReconstructionPath::HotPath,
        invocation: InvocationInput {
            host_function_xdr,
            resources_xdr,
            source_account_xdr,
            auth_entries_xdr,
            read_only_keys,
            read_write_keys,
            ttl_by_key,
            cpu_cost_params,
            mem_cost_params,
            network_id,
            protocol_version,
            timestamp,
            base_reserve: BASE_RESERVE_STROOPS,
            min_temp_entry_ttl: archival_settings.min_temporary_ttl,
            min_persistent_entry_ttl: archival_settings.min_persistent_ttl,
            max_entry_ttl: archival_settings.max_entry_ttl,
            cpu_instructions_limit: u64::from(soroban_data.resources.instructions),
        },
    })
}

async fn reconstruct_cold_path(
    ledger_sequence: u32,
    _network: &NetworkConfig,
) -> GratResult<LedgerState> {
    Err(GratError::ReplayError(format!(
        "Cold-path (archived) ledger reconstruction for ledger {ledger_sequence} is not yet \
         implemented — only transactions within the last {HOT_PATH_THRESHOLD} ledgers can be \
         replayed"
    )))
}

fn extract_transaction(envelope: &TransactionEnvelope) -> GratResult<&Transaction> {
    match envelope {
        TransactionEnvelope::Tx(TransactionV1Envelope { tx, .. }) => Ok(tx),
        TransactionEnvelope::TxFeeBump(fb) => match &fb.tx.inner_tx {
            FeeBumpTransactionInnerTx::Tx(TransactionV1Envelope { tx, .. }) => Ok(tx),
        },
        TransactionEnvelope::TxV0(_) => Err(GratError::ReplayError(
            "Transaction envelope is a pre-Soroban V0 envelope and cannot invoke a host function"
                .to_string(),
        )),
    }
}

fn extract_invoke_host_function_op(tx: &Transaction) -> GratResult<&InvokeHostFunctionOp> {
    tx.operations
        .iter()
        .find_map(|op| match &op.body {
            OperationBody::InvokeHostFunction(invoke) => Some(invoke),
            _ => None,
        })
        .ok_or(GratError::NotSorobanTransaction)
}

fn muxed_account_to_account_id(account: &MuxedAccount) -> AccountId {
    let ed25519 = match account {
        MuxedAccount::Ed25519(key) => key.clone(),
        MuxedAccount::MuxedEd25519(med) => med.ed25519.clone(),
    };
    AccountId(PublicKey::PublicKeyTypeEd25519(ed25519))
}

fn config_setting_key(config_setting_id: ConfigSettingId) -> LedgerKey {
    LedgerKey::ConfigSetting(LedgerKeyConfigSetting { config_setting_id })
}

fn encode_key(key: &LedgerKey) -> GratResult<String> {
    let bytes = encode_xdr(key, "LedgerKey")?;
    Ok(STANDARD.encode(bytes))
}

fn encode_keys<'a>(keys: impl Iterator<Item = &'a LedgerKey>) -> GratResult<Vec<String>> {
    keys.map(encode_key).collect()
}

pub(crate) fn encode_xdr<T: WriteXdr>(value: &T, type_name: &'static str) -> GratResult<Vec<u8>> {
    value
        .to_xdr(Limits::none())
        .map_err(|e| GratError::XdrDecodingFailed {
            type_name,
            reason: e.to_string(),
        })
}
