//! Structural diffs for Soroban contract storage entries.

use crate::types::trace::{DiffChangeType, LedgerEntryDiff, StateDiff};
use serde::Serialize;
use std::fmt::Debug;
use stellar_xdr::curr::{Asset, ContractCodeEntry, ContractDataEntry, ScMap, ScVal, TrustLineEntry};

/// Report a contract WASM code hash rotation.
///
/// The code hash uniquely identifies the deployed bytecode, so a changed hash
/// is reported as an update with the previous and replacement hashes in hex.
/// No entry is returned when the hashes are equal.
pub fn diff_contract_code_entry(old: &ContractCodeEntry, new: &ContractCodeEntry) -> StateDiff {
    let old_hash = hex::encode(old.hash.0);
    let new_hash = hex::encode(new.hash.0);

    let entries = if old.hash == new.hash {
        Vec::new()
    } else {
        vec![LedgerEntryDiff {
            key: "contract_code.hash".to_owned(),
            before: Some(old_hash),
            after: Some(new_hash),
            change_type: DiffChangeType::Updated,
        }]
    };

    StateDiff { entries }
}

/// Compute the changes to a contract storage entry's value.
///
/// Maps are compared by their XDR keys, vectors by index, and scalar values
/// directly. Each changed path is reported as a separate state-diff entry.
/// Unchanged values are omitted.
pub fn diff_contract_data(old: &ContractDataEntry, new: &ContractDataEntry) -> StateDiff {
    let mut entries = Vec::new();
    let root = format!("storage[{}]", json_string(&old.key));
    diff_value(&old.val, &new.val, &root, &mut entries);
    StateDiff { entries }
}

/// Compute the changes to a trustline entry.
///
/// Reports balance shifts, limit changes, authorization flag changes, and
/// liability changes for classic assets. The asset field is rendered in a
/// human-readable form (e.g. "USDC:ISSUER" or "native").
pub fn diff_trustline_entry(old: &TrustLineEntry, new: &TrustLineEntry) -> StateDiff {
    let mut entries = Vec::new();
    let asset = format_asset(&old.asset);
    let root = format!("trustline[{asset}]");

    if old.balance != new.balance {
        entries.push(LedgerEntryDiff {
            key: format!("{root}.balance"),
            before: Some(old.balance.to_string()),
            after: Some(new.balance.to_string()),
            change_type: DiffChangeType::Updated,
        });
    }

    if old.limit != new.limit {
        entries.push(LedgerEntryDiff {
            key: format!("{root}.limit"),
            before: Some(old.limit.to_string()),
            after: Some(new.limit.to_string()),
            change_type: DiffChangeType::Updated,
        });
    }

    if old.flags != new.flags {
        entries.push(LedgerEntryDiff {
            key: format!("{root}.flags"),
            before: Some(format!("{:?}", old.flags)),
            after: Some(format!("{:?}", new.flags)),
            change_type: DiffChangeType::Updated,
        });
    }

    if old.ext != new.ext {
        entries.push(LedgerEntryDiff {
            key: format!("{root}.ext"),
            before: Some(format!("{:?}", old.ext)),
            after: Some(format!("{:?}", new.ext)),
            change_type: DiffChangeType::Updated,
        });
    }

    StateDiff { entries }
}

fn format_asset(asset: &Asset) -> String {
    match asset {
        Asset::Native => "native".to_owned(),
        Asset::CreditAlphanum4(a) => {
            let code = String::from_utf8_lossy(&a.asset_code.0)
                .trim_end_matches('\0')
                .to_owned();
            format!("{code}:{}", a.issuer.to_string())
        }
        Asset::CreditAlphanum12(a) => {
            let code = String::from_utf8_lossy(&a.asset_code.0)
                .trim_end_matches('\0')
                .to_owned();
            format!("{code}:{}", a.issuer.to_string())
        }
    }
}

fn diff_value(old: &ScVal, new: &ScVal, path: &str, entries: &mut Vec<LedgerEntryDiff>) {
    match (old, new) {
        (ScVal::Map(Some(old_map)), ScVal::Map(Some(new_map))) => {
            diff_map(old_map, new_map, path, entries);
        }
        (ScVal::Vec(Some(old_vec)), ScVal::Vec(Some(new_vec))) => {
            let shared_len = old_vec.len().min(new_vec.len());
            for (index, (old_value, new_value)) in old_vec
                .iter()
                .zip(new_vec.iter())
                .take(shared_len)
                .enumerate()
            {
                diff_value(old_value, new_value, &format!("{path}[{index}]"), entries);
            }

            for (index, value) in old_vec.iter().enumerate().skip(shared_len) {
                push_change(
                    entries,
                    format!("{path}[{index}]"),
                    Some(value),
                    None,
                    DiffChangeType::Deleted,
                );
            }
            for (index, value) in new_vec.iter().enumerate().skip(shared_len) {
                push_change(
                    entries,
                    format!("{path}[{index}]"),
                    None,
                    Some(value),
                    DiffChangeType::Created,
                );
            }
        }
        _ if old != new => push_change(
            entries,
            path.to_owned(),
            Some(old),
            Some(new),
            DiffChangeType::Updated,
        ),
        _ => {}
    }
}

fn diff_map(old: &ScMap, new: &ScMap, path: &str, entries: &mut Vec<LedgerEntryDiff>) {
    let mut matched_new = vec![false; new.len()];

    for old_entry in old.iter() {
        if let Some((index, new_entry)) = new
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.key == old_entry.key)
        {
            matched_new[index] = true;
            diff_value(
                &old_entry.val,
                &new_entry.val,
                &format!("{path}[{}]", json_string(&old_entry.key)),
                entries,
            );
        } else {
            push_change(
                entries,
                format!("{path}[{}]", json_string(&old_entry.key)),
                Some(&old_entry.val),
                None,
                DiffChangeType::Deleted,
            );
        }
    }

    for (index, new_entry) in new.iter().enumerate() {
        if !matched_new[index] {
            push_change(
                entries,
                format!("{path}[{}]", json_string(&new_entry.key)),
                None,
                Some(&new_entry.val),
                DiffChangeType::Created,
            );
        }
    }
}

fn push_change(
    entries: &mut Vec<LedgerEntryDiff>,
    key: String,
    before: Option<&ScVal>,
    after: Option<&ScVal>,
    change_type: DiffChangeType,
) {
    entries.push(LedgerEntryDiff {
        key,
        before: before.map(json_string),
        after: after.map(json_string),
        change_type,
    });
}

fn json_string<T: Debug + Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

#[cfg(test)]
mod tests {
    use super::{diff_contract_code_entry, diff_contract_data, diff_trustline_entry};
    use crate::types::trace::DiffChangeType;
    use stellar_xdr::curr::{
        AccountId, Asset, AssetCode12, AssetCode4, ContractCodeEntry, ContractCodeEntryExt,
        ContractDataDurability, ContractDataEntry, ExtensionPoint, Hash, PublicKey, ScMapEntry,
        ScSymbol, ScVal, StringM, TrustLineEntry, TrustLineEntryExt, TrustLineFlags, Uint256,
    };

    fn contract_code(hash: [u8; 32]) -> ContractCodeEntry {
        ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: Hash(hash),
            code: Default::default(),
        }
    }

    fn contract_data(val: ScVal) -> ContractDataEntry {
        ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: stellar_xdr::curr::ScAddress::Contract(Hash([0; 32])),
            key: ScVal::Symbol(ScSymbol(StringM::try_from("state").expect("symbol"))),
            durability: ContractDataDurability::Persistent,
            val,
        }
    }

    fn symbol(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(StringM::try_from(value).expect("symbol")))
    }

    #[test]
    fn deeply_reports_added_deleted_and_modified_map_values() {
        let old = contract_data(ScVal::Map(Some(
            vec![ScMapEntry {
                key: symbol("account"),
                val: ScVal::Map(Some(
                    vec![
                        ScMapEntry {
                            key: symbol("balance"),
                            val: ScVal::I64(10),
                        },
                        ScMapEntry {
                            key: symbol("obsolete"),
                            val: ScVal::Bool(true),
                        },
                    ]
                    .try_into()
                    .expect("nested map"),
                )),
            }]
            .try_into()
            .expect("map"),
        )));
        let new = contract_data(ScVal::Map(Some(
            vec![ScMapEntry {
                key: symbol("account"),
                val: ScVal::Map(Some(
                    vec![
                        ScMapEntry {
                            key: symbol("balance"),
                            val: ScVal::I64(25),
                        },
                        ScMapEntry {
                            key: symbol("active"),
                            val: ScVal::Bool(true),
                        },
                    ]
                    .try_into()
                    .expect("nested map"),
                )),
            }]
            .try_into()
            .expect("map"),
        )));

        let diff = diff_contract_data(&old, &new);

        assert_eq!(diff.entries.len(), 3);
        assert!(matches!(
            diff.entries[0].change_type,
            DiffChangeType::Updated
        ));
        assert_eq!(diff.entries[0].before.as_deref(), Some(r#"{"i64":10}"#));
        assert_eq!(diff.entries[0].after.as_deref(), Some(r#"{"i64":25}"#));
        assert!(matches!(
            diff.entries[1].change_type,
            DiffChangeType::Deleted
        ));
        assert!(matches!(
            diff.entries[2].change_type,
            DiffChangeType::Created
        ));
        assert!(diff.entries[0].key.contains("balance"));
        assert!(diff.entries[1].key.contains("obsolete"));
        assert!(diff.entries[2].key.contains("active"));
    }

    #[test]
    fn equal_values_produce_no_changes() {
        let entry = contract_data(ScVal::Vec(Some(
            vec![ScVal::U32(7)].try_into().expect("vector"),
        )));

        assert!(diff_contract_data(&entry, &entry).entries.is_empty());
    }

    #[test]
    fn vector_length_changes_are_reported_by_index() {
        let old = contract_data(ScVal::Vec(Some(
            vec![ScVal::U32(1), ScVal::U32(2)]
                .try_into()
                .expect("vector"),
        )));
        let new = contract_data(ScVal::Vec(Some(
            vec![ScVal::U32(1), ScVal::U32(2), ScVal::U32(3)]
                .try_into()
                .expect("vector"),
        )));

        let diff = diff_contract_data(&old, &new);

        assert_eq!(diff.entries.len(), 1);
        assert!(matches!(
            diff.entries[0].change_type,
            DiffChangeType::Created
        ));
        assert!(diff.entries[0].key.ends_with("[2]"));
    }

    #[test]
    fn contract_code_hash_rotation_reports_before_and_after_hashes() {
        let old = contract_code([0x11; 32]);
        let new = contract_code([0x22; 32]);

        let diff = diff_contract_code_entry(&old, &new);
        let expected_old_hash = "11".repeat(32);
        let expected_new_hash = "22".repeat(32);

        assert_eq!(diff.entries.len(), 1);
        assert_eq!(diff.entries[0].key, "contract_code.hash");
        assert_eq!(
            diff.entries[0].before.as_deref(),
            Some(expected_old_hash.as_str())
        );
        assert_eq!(
            diff.entries[0].after.as_deref(),
            Some(expected_new_hash.as_str())
        );
        assert!(matches!(
            diff.entries[0].change_type,
            DiffChangeType::Updated
        ));
    }

    #[test]
    fn identical_contract_code_hashes_produce_no_changes() {
        let entry = contract_code([0x11; 32]);

        assert!(diff_contract_code_entry(&entry, &entry).entries.is_empty());
    }

    fn trustline(
        asset: Asset,
        balance: i64,
        limit: i64,
        flags: u32,
    ) -> TrustLineEntry {
        TrustLineEntry {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32]))),
            asset,
            balance,
            limit,
            flags: TrustLineFlags::from_bits_truncate(flags),
            ext: TrustLineEntryExt::V0,
        }
    }

    fn usdc_asset() -> Asset {
        Asset::CreditAlphanum4(AssetCode4(*b"USDC"))
    }

    #[test]
    fn trustline_balance_change_is_reported() {
        let old = trustline(usdc_asset(), 100, 1000, 0);
        let new = trustline(usdc_asset(), 250, 1000, 0);

        let diff = diff_trustline_entry(&old, &new);

        assert_eq!(diff.entries.len(), 1);
        assert!(diff.entries[0].key.contains("balance"));
        assert_eq!(diff.entries[0].before.as_deref(), Some("100"));
        assert_eq!(diff.entries[0].after.as_deref(), Some("250"));
        assert!(matches!(
            diff.entries[0].change_type,
            DiffChangeType::Updated
        ));
    }

    #[test]
    fn trustline_limit_and_flags_changes_are_reported() {
        let old = trustline(usdc_asset(), 100, 1000, 0);
        let new = trustline(usdc_asset(), 100, 5000, 1);

        let diff = diff_trustline_entry(&old, &new);

        assert_eq!(diff.entries.len(), 2);
        assert!(diff.entries.iter().any(|e| e.key.contains("limit")));
        assert!(diff.entries.iter().any(|e| e.key.contains("flags")));
    }

    #[test]
    fn trustline_asset_is_rendered_human_readably() {
        let old = trustline(usdc_asset(), 0, 1000, 0);
        let new = trustline(usdc_asset(), 1, 1000, 0);

        let diff = diff_trustline_entry(&old, &new);

        assert!(diff.entries[0].key.contains("USDC"));
    }

    #[test]
    fn trustline_alphanum12_asset_is_rendered() {
        let asset = Asset::CreditAlphanum12(AssetCode12(*b"LONGASSET\0\0\0"));
        let old = trustline(asset.clone(), 0, 1000, 0);
        let new = trustline(asset, 5, 1000, 0);

        let diff = diff_trustline_entry(&old, &new);

        assert!(diff.entries[0].key.contains("LONGASSET"));
    }

    #[test]
    fn trustline_native_asset_is_rendered() {
        let old = trustline(Asset::Native, 0, 1000, 0);
        let new = trustline(Asset::Native, 10, 1000, 0);

        let diff = diff_trustline_entry(&old, &new);

        assert!(diff.entries[0].key.contains("native"));
    }

    #[test]
    fn identical_trustlines_produce_no_changes() {
        let entry = trustline(usdc_asset(), 100, 1000, 0);

        assert!(diff_trustline_entry(&entry, &entry).entries.is_empty());
    }
}
