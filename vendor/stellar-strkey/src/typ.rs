pub const PUBLIC_KEY: u8 = 6 << 3;
pub const PRIVATE_KEY: u8 = 18 << 3;
pub const MUXED_ACCOUNT: u8 = 12 << 3;
pub const PRE_AUTH_TX: u8 = 19 << 3;
pub const HASH_X: u8 = 23 << 3;
pub const SIGNED_PAYLOAD: u8 = 15 << 3;
pub const CONTRACT: u8 = 2 << 3;

pub mod public_key_alg {
    pub const ED25519: u8 = 0;
}
