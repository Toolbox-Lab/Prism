use crate::typ::{self, public_key_alg::ED25519};

pub const PUBLIC_KEY_ED25519: u8 = typ::PUBLIC_KEY | ED25519;
pub const PRIVATE_KEY_ED25519: u8 = typ::PRIVATE_KEY | ED25519;
pub const MUXED_ACCOUNT_ED25519: u8 = typ::MUXED_ACCOUNT | ED25519;
pub const PRE_AUTH_TX: u8 = typ::PRE_AUTH_TX;
pub const HASH_X: u8 = typ::HASH_X;
pub const SIGNED_PAYLOAD_ED25519: u8 = typ::SIGNED_PAYLOAD | ED25519;
pub const CONTRACT: u8 = typ::CONTRACT;
