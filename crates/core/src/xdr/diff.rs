use std::fmt;
use stellar_xdr::next::{AccountEntry, Signer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntryDiff {
    pub balance_change: i64,
    pub seq_num_change: i64,
    pub num_sub_entries_change: i32,
    pub added_signers: Vec<Signer>,
    pub removed_signers: Vec<Signer>,
}

impl fmt::Display for AccountEntryDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "AccountEntry Diff:")?;

        if self.balance_change != 0 {
            let direction = if self.balance_change > 0 { "+" } else { "" };
            writeln!(
                f,
                "  💰 Balance: {}{} XLM (stroops)",
                direction, self.balance_change
            )?;
        }

        if self.seq_num_change != 0 {
            let direction = if self.seq_num_change > 0 { "+" } else { "" };
            writeln!(f, "  🔢 Sequence: {}{}", direction, self.seq_num_change)?;
        }

        if self.num_sub_entries_change != 0 {
            let direction = if self.num_sub_entries_change > 0 {
                "+"
            } else {
                ""
            };
            writeln!(
                f,
                "  📦 Sub-entries: {}{}",
                direction, self.num_sub_entries_change
            )?;
        }

        if !self.added_signers.is_empty() {
            writeln!(f, "  🔐 Added Signers: {}", self.added_signers.len())?;
            for signer in &self.added_signers {
                writeln!(f, "    - {signer:?}")?;
            }
        }

        if !self.removed_signers.is_empty() {
            writeln!(f, "  🔓 Removed Signers: {}", self.removed_signers.len())?;
            for signer in &self.removed_signers {
                writeln!(f, "    - {signer:?}")?;
            }
        }

        Ok(())
    }
}

pub fn diff_account_entry(old: &AccountEntry, new: &AccountEntry) -> AccountEntryDiff {
    let balance_change = new.balance - old.balance;
    let seq_num_change = new.seq_num.0 - old.seq_num.0;

    let old_sub = old.num_sub_entries.cast_signed();
    let new_sub = new.num_sub_entries.cast_signed();
    let num_sub_entries_change = new_sub - old_sub;

    let mut added_signers = Vec::new();
    let mut removed_signers = Vec::new();

    let old_signers = old.signers.as_vec();
    let new_signers = new.signers.as_vec();

    for signer in new_signers {
        if !old_signers.contains(signer) {
            added_signers.push(signer.clone());
        }
    }

    for signer in old_signers {
        if !new_signers.contains(signer) {
            removed_signers.push(signer.clone());
        }
    }

    AccountEntryDiff {
        balance_change,
        seq_num_change,
        num_sub_entries_change,
        added_signers,
        removed_signers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::next::{
        AccountEntryExt, AccountId, PublicKey, SequenceNumber, SignerKey, String32, StringM, Thresholds,
        Uint256, VecM,
    };

    fn dummy_account_entry() -> AccountEntry {
        AccountEntry {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32]))),
            balance: 1000,
            seq_num: SequenceNumber(10),
            num_sub_entries: 2,
            inflation_dest: None,
            flags: 0,
            home_domain: String32(StringM::default()),
            thresholds: Thresholds([0; 4]),
            signers: VecM::default(),
            ext: AccountEntryExt::V0,
        }
    }

    #[test]
    fn test_diff_balance_and_seq() {
        let old = dummy_account_entry();
        let mut new = old.clone();
        new.balance = 800; // 200 fee deduction
        new.seq_num = SequenceNumber(11); // +1 seq

        let diff = diff_account_entry(&old, &new);
        assert_eq!(diff.balance_change, -200);
        assert_eq!(diff.seq_num_change, 1);
        assert_eq!(diff.num_sub_entries_change, 0);
        assert!(diff.added_signers.is_empty());
        assert!(diff.removed_signers.is_empty());
    }

    #[test]
    fn test_diff_signers() {
        let mut old = dummy_account_entry();
        let mut new = dummy_account_entry();

        let signer1 = Signer {
            key: SignerKey::Ed25519(Uint256([1; 32])),
            weight: 1,
        };
        let signer2 = Signer {
            key: SignerKey::Ed25519(Uint256([2; 32])),
            weight: 2,
        };

        let mut old_signers = old.signers.to_vec();
        old_signers.push(signer1.clone());
        old.signers = old_signers.try_into().unwrap();

        let mut new_signers = new.signers.to_vec();
        new_signers.push(signer2.clone());
        new.signers = new_signers.try_into().unwrap();

        let diff = diff_account_entry(&old, &new);
        assert_eq!(diff.added_signers.len(), 1);
        assert_eq!(diff.added_signers[0], signer2);

        assert_eq!(diff.removed_signers.len(), 1);
        assert_eq!(diff.removed_signers[0], signer1);
    }
}
