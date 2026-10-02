use crate::decode::mappings::crypto;

#[test]
fn lookup_invalid_signature() {
    let detail = crypto::lookup(1).unwrap();

    assert_eq!(detail.name, "InvalidSignature");
    assert!(detail.summary.contains("Signature verification"));
}

#[test]
fn unknown_crypto_error() {
    assert!(crypto::lookup(999).is_none());
}
