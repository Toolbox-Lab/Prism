//! What-if speculative simulation.
//!
//! Replays a historical transaction twice against the same reconstructed
//! ledger state: once as it happened, and once with a set of [`WhatIfPatch`]es
//! applied (arguments, ledger entries, resource limits or signers). Both runs
//! go through `soroban-env-host` via the regular replay sandbox, so nothing is
//! redeployed and no network state is touched. The two traces are compared to
//! find the first point where execution diverges.

use crate::decode::json_to_scval;
use crate::error::{GratError, GratResult};
use crate::replay::sandbox::MEM_BYTES_CEILING;
use crate::replay::state::LedgerState;
use crate::types::trace::{ContractInvocation, DiffChangeType, ExecutionTrace};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use stellar_xdr::curr::{
    AccountId, Hash, HostFunction, LedgerEntry, LedgerEntryData, LedgerKey, Limits, PublicKey,
    ReadXdr, ScAddress, ScVal, SorobanAddressCredentials, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, SorobanResources,
    Uint256, VecM, WriteXdr,
};

/// How many ledgers past the replayed ledger a synthetic signer's auth entry
/// stays valid for. Only needs to outlive the single simulated invocation.
const SYNTHETIC_SIGNATURE_TTL_LEDGERS: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WhatIfPatch {
    /// Replace the `index`-th argument of the invoked contract function.
    ///
    /// `new_value` is an `ScVal`, given as stellar-xdr JSON (`{"u32":5}`),
    /// base64 `ScVal` XDR, or plain JSON (`5`, `"text"`, `[1,2]`, ...).
    ModifyArgument { index: usize, new_value: String },

    /// Override a ledger entry in the transaction footprint.
    ///
    /// `key` is a base64 `LedgerKey` XDR (or stellar-xdr JSON). `new_value` is
    /// a full `LedgerEntry` (base64 XDR or JSON) or, for contract data
    /// entries, just the new stored `ScVal` in any form `ModifyArgument`
    /// accepts.
    ModifyLedgerEntry { key: String, new_value: String },

    /// Replay under different CPU instruction and/or memory byte limits.
    ModifyResourceLimits {
        cpu_limit: Option<u64>,

        memory_limit: Option<u64>,
    },

    /// Add or remove an authorizing signer (`G...` account or `C...` contract).
    ///
    /// Removing drops every authorization entry whose credentials belong to
    /// the signer. Adding appends an address-credential entry for the root
    /// invocation with an empty signature, which shows how the contract
    /// behaves once that address is asked to authorize.
    ModifyAuth {
        add_signer: Option<String>,

        remove_signer: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatIfResult {
    pub original: ExecutionTrace,

    pub modified: ExecutionTrace,

    pub divergence_point: Option<usize>,

    pub summary: String,
}

/// A ledger state with patches applied, plus the memory ceiling to replay it
/// under (which lives outside [`LedgerState`]).
#[derive(Debug, Clone)]
pub struct PatchedState {
    pub state: LedgerState,
    pub memory_limit: u64,
}

pub async fn simulate_whatif(
    tx_hash: &str,
    patches: &[WhatIfPatch],
    network: &crate::types::config::NetworkConfig,
) -> GratResult<WhatIfResult> {
    if patches.is_empty() {
        return Err(GratError::Internal(
            "what-if simulation needs at least one patch".to_string(),
        ));
    }

    let ledger_state = crate::replay::state::reconstruct_state(tx_hash, network).await?;
    // Validate every patch before spending time on any replay.
    let patched = apply_patches(&ledger_state, patches)?;

    let original =
        crate::replay::replay_with_state(tx_hash, network, &ledger_state, MEM_BYTES_CEILING)
            .await?;
    let modified =
        crate::replay::replay_with_state(tx_hash, network, &patched.state, patched.memory_limit)
            .await?;

    let divergence_point = find_divergence(&original, &modified);
    let summary = summarize(patches, &original, &modified, divergence_point);

    Ok(WhatIfResult {
        original,
        modified,
        divergence_point,
        summary,
    })
}

/// Clones `state` and applies every patch in order.
///
/// Fails with a descriptive error, before any execution, when a patch does
/// not validate (malformed `ScVal`, invalid or out-of-footprint ledger key,
/// out-of-range argument index, unknown signer, zero limit, ...).
pub fn apply_patches(state: &LedgerState, patches: &[WhatIfPatch]) -> GratResult<PatchedState> {
    let mut patched = PatchedState {
        state: state.clone(),
        memory_limit: MEM_BYTES_CEILING,
    };

    for (i, patch) in patches.iter().enumerate() {
        let applied = match patch {
            WhatIfPatch::ModifyArgument { index, new_value } => {
                apply_argument(&mut patched.state, *index, new_value)
            }
            WhatIfPatch::ModifyLedgerEntry { key, new_value } => {
                apply_ledger_entry(&mut patched.state, key, new_value)
            }
            WhatIfPatch::ModifyResourceLimits {
                cpu_limit,
                memory_limit,
            } => apply_limits(&mut patched, *cpu_limit, *memory_limit),
            WhatIfPatch::ModifyAuth {
                add_signer,
                remove_signer,
            } => apply_auth(
                &mut patched.state,
                add_signer.as_deref(),
                remove_signer.as_deref(),
            ),
        };
        applied.map_err(|e| invalid(format!("patch #{i} ({}): {e}", patch_label(patch))))?;
    }

    Ok(patched)
}

fn patch_label(patch: &WhatIfPatch) -> &'static str {
    match patch {
        WhatIfPatch::ModifyArgument { .. } => "ModifyArgument",
        WhatIfPatch::ModifyLedgerEntry { .. } => "ModifyLedgerEntry",
        WhatIfPatch::ModifyResourceLimits { .. } => "ModifyResourceLimits",
        WhatIfPatch::ModifyAuth { .. } => "ModifyAuth",
    }
}

fn invalid(msg: String) -> GratError {
    GratError::ReplayError(format!("invalid what-if patch: {msg}"))
}

type PatchResult = Result<(), String>;

// ---------------------------------------------------------------------------
// Value parsing
// ---------------------------------------------------------------------------

fn decode_b64_xdr<T: ReadXdr>(input: &str) -> Option<T> {
    let bytes = STANDARD.decode(input.trim()).ok()?;
    T::from_xdr(bytes, Limits::none()).ok()
}

fn to_xdr<T: WriteXdr>(value: &T, what: &str) -> Result<Vec<u8>, String> {
    value
        .to_xdr(Limits::none())
        .map_err(|e| format!("failed to encode {what}: {e}"))
}

/// Parses an `ScVal` from stellar-xdr JSON, base64 XDR, or plain JSON.
pub fn parse_scval(input: &str) -> Result<ScVal, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("value is empty".to_string());
    }
    if let Ok(val) = serde_json::from_str::<ScVal>(trimmed) {
        return Ok(val);
    }
    if let Some(val) = decode_b64_xdr::<ScVal>(trimmed) {
        return Ok(val);
    }
    let json: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
        format!("malformed ScVal: not stellar-xdr JSON, base64 XDR or valid JSON ({e})")
    })?;
    json_to_scval(&json).map_err(|e| format!("malformed ScVal JSON: {e}"))
}

fn parse_ledger_key(input: &str) -> Result<LedgerKey, String> {
    let trimmed = input.trim();
    decode_b64_xdr::<LedgerKey>(trimmed)
        .or_else(|| serde_json::from_str::<LedgerKey>(trimmed).ok())
        .ok_or_else(|| {
            format!("invalid ledger key '{trimmed}': expected base64 LedgerKey XDR or JSON")
        })
}

fn parse_ledger_entry(input: &str) -> Option<LedgerEntry> {
    let trimmed = input.trim();
    decode_b64_xdr::<LedgerEntry>(trimmed)
        .or_else(|| serde_json::from_str::<LedgerEntry>(trimmed).ok())
}

/// Parses a `G...` account or `C...` contract strkey into an `ScAddress`.
pub fn parse_address(input: &str) -> Result<ScAddress, String> {
    match stellar_strkey::Strkey::from_string(input.trim()) {
        Ok(stellar_strkey::Strkey::PublicKeyEd25519(pk)) => Ok(ScAddress::Account(AccountId(
            PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)),
        ))),
        Ok(stellar_strkey::Strkey::Contract(c)) => Ok(ScAddress::Contract(Hash(c.0))),
        _ => Err(format!(
            "invalid signer '{}': expected a G... account or C... contract address",
            input.trim()
        )),
    }
}

// ---------------------------------------------------------------------------
// Patches
// ---------------------------------------------------------------------------

fn decode_host_function(state: &LedgerState) -> Result<HostFunction, String> {
    HostFunction::from_xdr(&state.invocation.host_function_xdr, Limits::none())
        .map_err(|e| format!("failed to decode transaction host function: {e}"))
}

fn decode_auth_entries(state: &LedgerState) -> Result<Vec<SorobanAuthorizationEntry>, String> {
    state
        .invocation
        .auth_entries_xdr
        .iter()
        .map(|bytes| {
            SorobanAuthorizationEntry::from_xdr(bytes, Limits::none())
                .map_err(|e| format!("failed to decode authorization entry: {e}"))
        })
        .collect()
}

fn encode_auth_entries(
    state: &mut LedgerState,
    entries: &[SorobanAuthorizationEntry],
) -> PatchResult {
    state.invocation.auth_entries_xdr = entries
        .iter()
        .map(|e| to_xdr(e, "authorization entry"))
        .collect::<Result<_, _>>()?;
    Ok(())
}

fn apply_argument(state: &mut LedgerState, index: usize, new_value: &str) -> PatchResult {
    let new_val = parse_scval(new_value)?;
    let HostFunction::InvokeContract(mut call) = decode_host_function(state)? else {
        return Err(
            "transaction does not invoke a contract function, so it has no arguments".to_string(),
        );
    };

    let original_call = call.clone();
    let mut args: Vec<ScVal> = call.args.to_vec();
    if index >= args.len() {
        return Err(format!(
            "argument index {index} out of range: '{}' takes {} argument(s)",
            String::from_utf8_lossy(call.function_name.0.as_slice()),
            args.len()
        ));
    }
    args[index] = new_val;
    call.args = args
        .try_into()
        .map_err(|_| "too many arguments for XDR".to_string())?;

    // Keep matching authorization entries in sync, otherwise the patched
    // call would simply fail authorization for the old arguments.
    let mut entries = decode_auth_entries(state)?;
    let mut changed = false;
    for entry in &mut entries {
        if let SorobanAuthorizedFunction::ContractFn(auth_call) =
            &mut entry.root_invocation.function
        {
            if *auth_call == original_call {
                *auth_call = call.clone();
                changed = true;
            }
        }
    }
    if changed {
        encode_auth_entries(state, &entries)?;
    }

    state.invocation.host_function_xdr =
        to_xdr(&HostFunction::InvokeContract(call), "host function")?;
    Ok(())
}

fn apply_ledger_entry(state: &mut LedgerState, key: &str, new_value: &str) -> PatchResult {
    let ledger_key = parse_ledger_key(key)?;
    // Re-encode so differently formatted inputs map onto the state's keys.
    let key_b64 = STANDARD.encode(to_xdr(&ledger_key, "ledger key")?);

    let inv = &state.invocation;
    if !inv.read_only_keys.contains(&key_b64) && !inv.read_write_keys.contains(&key_b64) {
        return Err(format!(
            "ledger key {key_b64} is not in the transaction footprint; only footprint entries \
             can be patched"
        ));
    }

    let entry = match parse_ledger_entry(new_value) {
        Some(entry) => entry,
        None => {
            let val = parse_scval(new_value).map_err(|e| {
                format!("new_value is neither a LedgerEntry nor a contract data ScVal: {e}")
            })?;
            contract_data_with_value(state, &ledger_key, &key_b64, val)?
        }
    };

    if !entry_matches_key(&entry, &ledger_key) {
        return Err(format!(
            "new ledger entry type {} does not match ledger key type {}",
            entry.data.name(),
            ledger_key.name()
        ));
    }

    state
        .entries
        .insert(key_b64, to_xdr(&entry, "ledger entry")?);
    Ok(())
}

fn contract_data_with_value(
    state: &LedgerState,
    ledger_key: &LedgerKey,
    key_b64: &str,
    val: ScVal,
) -> Result<LedgerEntry, String> {
    let LedgerKey::ContractData(_) = ledger_key else {
        return Err(
            "a bare ScVal can only patch a contract data entry; pass a full LedgerEntry instead"
                .to_string(),
        );
    };
    let existing = state.entries.get(key_b64).ok_or_else(|| {
        format!("contract data entry {key_b64} does not exist yet; pass a full LedgerEntry")
    })?;
    let mut entry = LedgerEntry::from_xdr(existing, Limits::none())
        .map_err(|e| format!("failed to decode existing ledger entry: {e}"))?;
    match &mut entry.data {
        LedgerEntryData::ContractData(data) => data.val = val,
        _ => return Err("existing entry is not contract data".to_string()),
    }
    Ok(entry)
}

fn entry_matches_key(entry: &LedgerEntry, key: &LedgerKey) -> bool {
    match (&entry.data, key) {
        (LedgerEntryData::ContractData(d), LedgerKey::ContractData(k)) => {
            d.contract == k.contract && d.key == k.key && d.durability == k.durability
        }
        (LedgerEntryData::ContractCode(d), LedgerKey::ContractCode(k)) => d.hash == k.hash,
        (LedgerEntryData::Account(d), LedgerKey::Account(k)) => d.account_id == k.account_id,
        (LedgerEntryData::Trustline(d), LedgerKey::Trustline(k)) => {
            d.account_id == k.account_id && d.asset == k.asset
        }
        (data, key) => data.name() == key.name(),
    }
}

fn apply_limits(
    patched: &mut PatchedState,
    cpu_limit: Option<u64>,
    memory_limit: Option<u64>,
) -> PatchResult {
    if cpu_limit.is_none() && memory_limit.is_none() {
        return Err("set cpu_limit, memory_limit, or both".to_string());
    }
    if let Some(cpu) = cpu_limit {
        if cpu == 0 {
            return Err("cpu_limit must be greater than zero".to_string());
        }
        let instructions = u32::try_from(cpu).map_err(|_| {
            format!(
                "cpu_limit {cpu} exceeds the protocol maximum of {}",
                u32::MAX
            )
        })?;
        let inv = &mut patched.state.invocation;
        let mut resources = SorobanResources::from_xdr(&inv.resources_xdr, Limits::none())
            .map_err(|e| format!("failed to decode transaction resources: {e}"))?;
        resources.instructions = instructions;
        inv.resources_xdr = to_xdr(&resources, "transaction resources")?;
        inv.cpu_instructions_limit = cpu;
    }
    if let Some(mem) = memory_limit {
        if mem == 0 {
            return Err("memory_limit must be greater than zero".to_string());
        }
        patched.memory_limit = mem;
    }
    Ok(())
}

fn credentials_address(entry: &SorobanAuthorizationEntry) -> Option<&ScAddress> {
    match &entry.credentials {
        SorobanCredentials::Address(creds) => Some(&creds.address),
        SorobanCredentials::SourceAccount => None,
    }
}

fn source_address(state: &LedgerState) -> Option<ScAddress> {
    AccountId::from_xdr(&state.invocation.source_account_xdr, Limits::none())
        .ok()
        .map(ScAddress::Account)
}

fn apply_auth(state: &mut LedgerState, add: Option<&str>, remove: Option<&str>) -> PatchResult {
    if add.is_none() && remove.is_none() {
        return Err("set add_signer, remove_signer, or both".to_string());
    }
    let mut entries = decode_auth_entries(state)?;

    if let Some(signer) = remove {
        let addr = parse_address(signer)?;
        let is_source = source_address(state).as_ref() == Some(&addr);
        let before = entries.len();
        entries.retain(|e| match credentials_address(e) {
            Some(a) => *a != addr,
            None => !is_source,
        });
        if entries.len() == before {
            return Err(format!(
                "signer {} has no authorization entry in this transaction",
                signer.trim()
            ));
        }
    }

    if let Some(signer) = add {
        let addr = parse_address(signer)?;
        if entries
            .iter()
            .any(|e| credentials_address(e) == Some(&addr))
        {
            return Err(format!(
                "signer {} already authorizes this transaction",
                signer.trim()
            ));
        }
        let root_invocation = match entries.first() {
            Some(entry) => entry.root_invocation.clone(),
            None => match decode_host_function(state)? {
                HostFunction::InvokeContract(call) => SorobanAuthorizedInvocation {
                    function: SorobanAuthorizedFunction::ContractFn(call),
                    sub_invocations: VecM::default(),
                },
                _ => {
                    return Err(
                        "cannot add a signer: transaction has no auth entries and does not \
                         invoke a contract function"
                            .to_string(),
                    )
                }
            },
        };
        let digest: [u8; 32] = Sha256::digest(signer.trim().as_bytes()).into();
        let mut nonce_bytes = [0u8; 8];
        nonce_bytes.copy_from_slice(&digest[..8]);
        entries.push(SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: addr,
                nonce: i64::from_be_bytes(nonce_bytes),
                signature_expiration_ledger: state
                    .ledger_sequence
                    .saturating_add(SYNTHETIC_SIGNATURE_TTL_LEDGERS),
                signature: ScVal::Void,
            }),
            root_invocation,
        });
    }

    encode_auth_entries(state, &entries)
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// Flattens a trace into its ordered execution steps: each contract
/// invocation entry, each host function call inside it, and each exit.
pub fn execution_steps(trace: &ExecutionTrace) -> Vec<String> {
    fn walk(inv: &ContractInvocation, out: &mut Vec<String>) {
        out.push(format!(
            "invoke {}::{}{}",
            inv.contract_id,
            inv.function_name,
            if inv.is_error { " (error)" } else { "" }
        ));
        for call in &inv.host_calls {
            out.push(format!(
                "host {}{}",
                call.function_name,
                if call.is_error { " (error)" } else { "" }
            ));
        }
        for sub in &inv.sub_invocations {
            walk(sub, out);
        }
        out.push(format!("return {}::{}", inv.contract_id, inv.function_name));
    }

    let mut steps = Vec::new();
    for inv in &trace.invocations {
        walk(inv, &mut steps);
    }
    steps
}

/// Index of the first execution step where the two traces differ, or `None`
/// when they executed identically.
pub fn find_divergence(original: &ExecutionTrace, modified: &ExecutionTrace) -> Option<usize> {
    let a = execution_steps(original);
    let b = execution_steps(modified);
    a.iter()
        .zip(b.iter())
        .position(|(x, y)| x != y)
        .or(if a.len() == b.len() {
            None
        } else {
            Some(a.len().min(b.len()))
        })
}

fn final_values(trace: &ExecutionTrace) -> BTreeMap<&str, Option<&str>> {
    trace
        .state_diff
        .entries
        .iter()
        .map(|e| {
            let after = match e.change_type {
                DiffChangeType::Deleted => None,
                _ => e.after.as_deref(),
            };
            (e.key.as_str(), after)
        })
        .collect()
}

fn signed_delta(before: u64, after: u64) -> String {
    if after >= before {
        format!("+{}", after - before)
    } else {
        format!("-{}", before - after)
    }
}

/// Human-readable comparison of the two runs.
pub fn summarize(
    patches: &[WhatIfPatch],
    original: &ExecutionTrace,
    modified: &ExecutionTrace,
    divergence_point: Option<usize>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "What-if simulation of {} with {} patch(es): {}",
        original.tx_hash,
        patches.len(),
        patches
            .iter()
            .map(patch_label)
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let original_steps = execution_steps(original);
    let modified_steps = execution_steps(modified);
    match divergence_point {
        None => lines.push(format!(
            "Execution path unchanged ({} steps).",
            original_steps.len()
        )),
        Some(i) => lines.push(format!(
            "Execution diverged at step {i}: original `{}`, modified `{}` ({} vs {} steps).",
            original_steps.get(i).map_or("<end>", String::as_str),
            modified_steps.get(i).map_or("<end>", String::as_str),
            original_steps.len(),
            modified_steps.len()
        )),
    }

    let (ro, rm) = (&original.resource_profile, &modified.resource_profile);
    lines.push(format!(
        "CPU instructions: {} -> {} ({}); memory bytes: {} -> {} ({}).",
        ro.total_cpu,
        rm.total_cpu,
        signed_delta(ro.total_cpu, rm.total_cpu),
        ro.total_memory,
        rm.total_memory,
        signed_delta(ro.total_memory, rm.total_memory)
    ));

    let (fo, fm) = (final_values(original), final_values(modified));
    let mut keys: Vec<&str> = fo.keys().chain(fm.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();
    let differing: Vec<&str> = keys
        .into_iter()
        .filter(|k| fo.get(k).copied().flatten() != fm.get(k).copied().flatten())
        .collect();
    if differing.is_empty() {
        lines.push("Final ledger state identical.".to_string());
    } else {
        lines.push(format!(
            "Final ledger state differs for {} entr{}: {}",
            differing.len(),
            if differing.len() == 1 { "y" } else { "ies" },
            differing.join(", ")
        ));
    }

    if original.diagnostic_events.len() != modified.diagnostic_events.len() {
        lines.push(format!(
            "Diagnostic events: {} -> {}.",
            original.diagnostic_events.len(),
            modified.diagnostic_events.len()
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::state::{InvocationInput, ReconstructionPath};
    use crate::types::trace::{
        HostFunctionCall, LedgerEntryDiff, ResourceProfile, StateDiff, TraceId,
    };
    use std::collections::HashMap;
    use stellar_xdr::curr::{
        ContractCostParams, ContractDataDurability, ContractDataEntry, ExtensionPoint,
        InvokeContractArgs, LedgerEntryExt, LedgerFootprint, LedgerKeyContractData, ScSymbol,
    };

    const SIGNER: &str = "GAYCUYT553C5LHVE2XPW5GMEJT4BXGM7AHMJWLAPZP53KJO7EIQADRSI";

    fn contract() -> ScAddress {
        ScAddress::Contract(Hash([7; 32]))
    }

    fn data_key() -> LedgerKey {
        LedgerKey::ContractData(LedgerKeyContractData {
            contract: contract(),
            key: ScVal::Symbol(ScSymbol("balance".try_into().unwrap())),
            durability: ContractDataDurability::Persistent,
        })
    }

    fn data_entry(val: ScVal) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 1,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: contract(),
                key: ScVal::Symbol(ScSymbol("balance".try_into().unwrap())),
                durability: ContractDataDurability::Persistent,
                val,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn b64<T: WriteXdr>(v: &T) -> String {
        STANDARD.encode(v.to_xdr(Limits::none()).unwrap())
    }

    fn call() -> InvokeContractArgs {
        InvokeContractArgs {
            contract_address: contract(),
            function_name: ScSymbol("transfer".try_into().unwrap()),
            args: vec![ScVal::U32(1), ScVal::U32(2)].try_into().unwrap(),
        }
    }

    fn signer_address() -> ScAddress {
        parse_address(SIGNER).unwrap()
    }

    fn state() -> LedgerState {
        let auth = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: signer_address(),
                nonce: 1,
                signature_expiration_ledger: 200,
                signature: ScVal::Void,
            }),
            root_invocation: SorobanAuthorizedInvocation {
                function: SorobanAuthorizedFunction::ContractFn(call()),
                sub_invocations: VecM::default(),
            },
        };
        let resources = SorobanResources {
            footprint: LedgerFootprint {
                read_only: VecM::default(),
                read_write: vec![data_key()].try_into().unwrap(),
            },
            instructions: 1_000_000,
            read_bytes: 0,
            write_bytes: 0,
        };
        let key_b64 = b64(&data_key());
        LedgerState {
            ledger_sequence: 100,
            entries: HashMap::from([(
                key_b64.clone(),
                data_entry(ScVal::U32(5)).to_xdr(Limits::none()).unwrap(),
            )]),
            reconstruction_path: ReconstructionPath::HotPath,
            invocation: InvocationInput {
                host_function_xdr: HostFunction::InvokeContract(call())
                    .to_xdr(Limits::none())
                    .unwrap(),
                resources_xdr: resources.to_xdr(Limits::none()).unwrap(),
                source_account_xdr: match signer_address() {
                    ScAddress::Account(a) => a.to_xdr(Limits::none()).unwrap(),
                    ScAddress::Contract(_) => unreachable!(),
                },
                auth_entries_xdr: vec![auth.to_xdr(Limits::none()).unwrap()],
                read_only_keys: Vec::new(),
                read_write_keys: vec![key_b64],
                ttl_by_key: HashMap::new(),
                cpu_cost_params: ContractCostParams(VecM::default()),
                mem_cost_params: ContractCostParams(VecM::default()),
                network_id: [0; 32],
                protocol_version: 21,
                timestamp: 0,
                base_reserve: 5_000_000,
                min_temp_entry_ttl: 16,
                min_persistent_entry_ttl: 4096,
                max_entry_ttl: 6_312_000,
                cpu_instructions_limit: 1_000_000,
            },
        }
    }

    fn host_args(s: &LedgerState) -> Vec<ScVal> {
        match decode_host_function(s).unwrap() {
            HostFunction::InvokeContract(c) => c.args.to_vec(),
            _ => unreachable!(),
        }
    }

    fn err_of(patches: &[WhatIfPatch]) -> String {
        apply_patches(&state(), patches).unwrap_err().to_string()
    }

    #[test]
    fn modify_argument_accepts_plain_xdr_json_and_base64() {
        for (input, expected) in [
            ("42", ScVal::U32(42)),
            (r#"{"i64":-7}"#, ScVal::I64(-7)),
            (b64(&ScVal::Bool(true)).as_str(), ScVal::Bool(true)),
        ] {
            let p = apply_patches(
                &state(),
                &[WhatIfPatch::ModifyArgument {
                    index: 1,
                    new_value: input.to_string(),
                }],
            )
            .unwrap();
            assert_eq!(host_args(&p.state), vec![ScVal::U32(1), expected.clone()]);
            // Matching auth entry is kept in sync with the new arguments.
            let entries = decode_auth_entries(&p.state).unwrap();
            match &entries[0].root_invocation.function {
                SorobanAuthorizedFunction::ContractFn(c) => {
                    assert_eq!(c.args.to_vec(), vec![ScVal::U32(1), expected])
                }
                _ => panic!("unexpected auth function"),
            }
        }
    }

    #[test]
    fn modify_argument_rejects_bad_index_and_malformed_value() {
        let e = err_of(&[WhatIfPatch::ModifyArgument {
            index: 5,
            new_value: "1".into(),
        }]);
        assert!(e.contains("out of range"), "{e}");
        let e = err_of(&[WhatIfPatch::ModifyArgument {
            index: 0,
            new_value: "{not json".into(),
        }]);
        assert!(e.contains("malformed ScVal"), "{e}");
        assert!(e.contains("patch #0 (ModifyArgument)"), "{e}");
    }

    #[test]
    fn modify_ledger_entry_with_scval_or_full_entry() {
        let key = b64(&data_key());
        let p = apply_patches(
            &state(),
            &[WhatIfPatch::ModifyLedgerEntry {
                key: key.clone(),
                new_value: "999".into(),
            }],
        )
        .unwrap();
        let entry = LedgerEntry::from_xdr(&p.state.entries[&key], Limits::none()).unwrap();
        assert_eq!(entry, data_entry(ScVal::U32(999)));

        let full = data_entry(ScVal::U64(3));
        let p = apply_patches(
            &state(),
            &[WhatIfPatch::ModifyLedgerEntry {
                key: key.clone(),
                new_value: b64(&full),
            }],
        )
        .unwrap();
        let entry = LedgerEntry::from_xdr(&p.state.entries[&key], Limits::none()).unwrap();
        assert_eq!(entry, full);
    }

    #[test]
    fn modify_ledger_entry_validates_key() {
        let e = err_of(&[WhatIfPatch::ModifyLedgerEntry {
            key: "not-a-key".into(),
            new_value: "1".into(),
        }]);
        assert!(e.contains("invalid ledger key"), "{e}");

        let other = LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(Hash([9; 32])),
            key: ScVal::U32(0),
            durability: ContractDataDurability::Temporary,
        });
        let e = err_of(&[WhatIfPatch::ModifyLedgerEntry {
            key: b64(&other),
            new_value: "1".into(),
        }]);
        assert!(e.contains("not in the transaction footprint"), "{e}");
    }

    #[test]
    fn modify_resource_limits() {
        let p = apply_patches(
            &state(),
            &[WhatIfPatch::ModifyResourceLimits {
                cpu_limit: Some(500),
                memory_limit: Some(1024),
            }],
        )
        .unwrap();
        assert_eq!(p.state.invocation.cpu_instructions_limit, 500);
        assert_eq!(p.memory_limit, 1024);
        let res =
            SorobanResources::from_xdr(&p.state.invocation.resources_xdr, Limits::none()).unwrap();
        assert_eq!(res.instructions, 500);

        let e = err_of(&[WhatIfPatch::ModifyResourceLimits {
            cpu_limit: Some(0),
            memory_limit: None,
        }]);
        assert!(e.contains("greater than zero"), "{e}");
    }

    #[test]
    fn modify_auth_remove_and_add() {
        let removed = apply_patches(
            &state(),
            &[WhatIfPatch::ModifyAuth {
                add_signer: None,
                remove_signer: Some(SIGNER.into()),
            }],
        )
        .unwrap();
        assert!(removed.state.invocation.auth_entries_xdr.is_empty());

        let contract_signer = stellar_strkey::Contract([3; 32]).to_string();
        let added = apply_patches(
            &state(),
            &[WhatIfPatch::ModifyAuth {
                add_signer: Some(contract_signer.clone()),
                remove_signer: None,
            }],
        )
        .unwrap();
        let entries = decode_auth_entries(&added.state).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            credentials_address(&entries[1]),
            Some(&ScAddress::Contract(Hash([3; 32])))
        );
        assert_eq!(entries[1].root_invocation, entries[0].root_invocation);

        let e = err_of(&[WhatIfPatch::ModifyAuth {
            add_signer: Some("bogus".into()),
            remove_signer: None,
        }]);
        assert!(e.contains("invalid signer"), "{e}");
    }

    fn trace(functions: &[&str], cpu: u64, balance: &str) -> ExecutionTrace {
        let invocation = ContractInvocation {
            trace_id: TraceId("t".into()),
            contract_id: "C1".into(),
            function_name: "transfer".into(),
            arguments: Vec::new(),
            return_value: None,
            host_calls: functions
                .iter()
                .map(|f| HostFunctionCall {
                    function_name: (*f).into(),
                    arguments: Vec::new(),
                    return_value: None,
                    cpu_instructions: 0,
                    memory_bytes: 0,
                    is_error: false,
                    error: None,
                })
                .collect(),
            sub_invocations: Vec::new(),
            total_cpu_instructions: 0,
            total_memory_bytes: 0,
            is_error: false,
        };
        ExecutionTrace {
            tx_hash: "abc".into(),
            ledger_sequence: 100,
            network: "testnet".into(),
            invocations: vec![invocation],
            state_diff: StateDiff {
                entries: vec![LedgerEntryDiff {
                    key: "k".into(),
                    before: Some("00".into()),
                    after: Some(balance.into()),
                    change_type: DiffChangeType::Updated,
                }],
            },
            resource_profile: ResourceProfile {
                total_cpu: cpu,
                ..ResourceProfile::default()
            },
            diagnostic_events: Vec::new(),
            auth_chains: Vec::new(),
        }
    }

    #[test]
    fn divergence_and_summary() {
        let a = trace(&["get", "put"], 100, "01");
        assert_eq!(find_divergence(&a, &a.clone()), None);

        let b = trace(&["get", "fail"], 80, "02");
        assert_eq!(find_divergence(&a, &b), Some(2));

        let c = trace(&["get"], 50, "01");
        assert_eq!(find_divergence(&a, &c), Some(2));

        let patches = [WhatIfPatch::ModifyResourceLimits {
            cpu_limit: Some(1),
            memory_limit: None,
        }];
        let summary = summarize(&patches, &a, &b, find_divergence(&a, &b));
        assert!(summary.contains("diverged at step 2"), "{summary}");
        assert!(summary.contains("host put"), "{summary}");
        assert!(summary.contains("100 -> 80 (-20)"), "{summary}");
        assert!(summary.contains("differs for 1 entry: k"), "{summary}");

        let same = summarize(&patches, &a, &a, None);
        assert!(same.contains("Execution path unchanged"), "{same}");
        assert!(same.contains("Final ledger state identical"), "{same}");
    }

    #[test]
    fn requires_at_least_one_valid_field() {
        let e = err_of(&[WhatIfPatch::ModifyAuth {
            add_signer: None,
            remove_signer: None,
        }]);
        assert!(e.contains("add_signer, remove_signer"), "{e}");
    }
}
