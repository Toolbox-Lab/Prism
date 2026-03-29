use std::{fmt::Debug, fmt::Display, str::FromStr};

use crate::{
    convert::{decode, encode},
    ed25519,
    error::DecodeError,
    version,
};

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Strkey {
    PublicKeyEd25519(ed25519::PublicKey),
    PrivateKeyEd25519(ed25519::PrivateKey),
    PreAuthTx(PreAuthTx),
    HashX(HashX),
    MuxedAccountEd25519(ed25519::MuxedAccount),
    SignedPayloadEd25519(ed25519::SignedPayload),
    Contract(Contract),
}

impl Strkey {
    pub fn to_string(&self) -> String {
        match self {
            Self::PublicKeyEd25519(x) => x.to_string(),
            Self::PrivateKeyEd25519(x) => x.to_string(),
            Self::PreAuthTx(x) => x.to_string(),
            Self::HashX(x) => x.to_string(),
            Self::MuxedAccountEd25519(x) => x.to_string(),
            Self::SignedPayloadEd25519(x) => x.to_string(),
            Self::Contract(x) => x.to_string(),
        }
    }

    pub fn from_string(s: &str) -> Result<Self, DecodeError> {
        let (ver, payload) = decode(s)?;
        match ver {
            version::PUBLIC_KEY_ED25519 => Ok(Self::PublicKeyEd25519(
                ed25519::PublicKey::from_payload(&payload)?,
            )),
            version::PRIVATE_KEY_ED25519 => Ok(Self::PrivateKeyEd25519(
                ed25519::PrivateKey::from_payload(&payload)?,
            )),
            version::PRE_AUTH_TX => Ok(Self::PreAuthTx(PreAuthTx::from_payload(&payload)?)),
            version::HASH_X => Ok(Self::HashX(HashX::from_payload(&payload)?)),
            version::MUXED_ACCOUNT_ED25519 => Ok(Self::MuxedAccountEd25519(
                ed25519::MuxedAccount::from_payload(&payload)?,
            )),
            version::SIGNED_PAYLOAD_ED25519 => Ok(Self::SignedPayloadEd25519(
                ed25519::SignedPayload::from_payload(&payload)?,
            )),
            version::CONTRACT => Ok(Self::Contract(Contract::from_payload(&payload)?)),
            _ => Err(DecodeError::Invalid),
        }
    }
}

impl Display for Strkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for Strkey {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Strkey::from_string(s)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreAuthTx(pub [u8; 32]);

impl Debug for PreAuthTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PreAuthTx(")?;
        write!(
            f,
            "{}",
            &self
                .0
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )?;
        write!(f, ")")?;
        Ok(())
    }
}

impl PreAuthTx {
    pub fn to_string(&self) -> String {
        encode(version::PRE_AUTH_TX, &self.0)
    }

    fn from_payload(payload: &[u8]) -> Result<Self, DecodeError> {
        Ok(Self(payload.try_into().map_err(|_| DecodeError::Invalid)?))
    }

    pub fn from_string(s: &str) -> Result<Self, DecodeError> {
        let (ver, payload) = decode(s)?;
        match ver {
            version::PRE_AUTH_TX => Self::from_payload(&payload),
            _ => Err(DecodeError::Invalid),
        }
    }
}

impl Display for PreAuthTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for PreAuthTx {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PreAuthTx::from_string(s)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct HashX(pub [u8; 32]);

impl Debug for HashX {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HashX(")?;
        write!(
            f,
            "{}",
            &self
                .0
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )?;
        write!(f, ")")?;
        Ok(())
    }
}

impl HashX {
    pub fn to_string(&self) -> String {
        encode(version::HASH_X, &self.0)
    }

    fn from_payload(payload: &[u8]) -> Result<Self, DecodeError> {
        Ok(Self(payload.try_into().map_err(|_| DecodeError::Invalid)?))
    }

    pub fn from_string(s: &str) -> Result<Self, DecodeError> {
        let (ver, payload) = decode(s)?;
        match ver {
            version::HASH_X => Self::from_payload(&payload),
            _ => Err(DecodeError::Invalid),
        }
    }
}

impl Display for HashX {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for HashX {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        HashX::from_string(s)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Contract(pub [u8; 32]);

impl Debug for Contract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Contract(")?;
        write!(
            f,
            "{}",
            &self
                .0
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )?;
        write!(f, ")")?;
        Ok(())
    }
}

impl Contract {
    pub fn to_string(&self) -> String {
        encode(version::CONTRACT, &self.0)
    }

    fn from_payload(payload: &[u8]) -> Result<Self, DecodeError> {
        Ok(Self(payload.try_into().map_err(|_| DecodeError::Invalid)?))
    }

    pub fn from_string(s: &str) -> Result<Self, DecodeError> {
        let (ver, payload) = decode(s)?;
        match ver {
            version::CONTRACT => Self::from_payload(&payload),
            _ => Err(DecodeError::Invalid),
        }
    }
}

impl Display for Contract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for Contract {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Contract::from_string(s)
    }
}
