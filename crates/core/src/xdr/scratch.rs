use stellar_xdr::next::{AccountEntry, SequenceNumber};

pub fn check_fields(entry: &AccountEntry) {
    let _bal = entry.balance;
    let _seq = entry.seq_num;
    let _num = entry.num_sub_entries;
    let _sig = &entry.signers;
}
