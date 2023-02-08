use super::*;

use core::cmp::{Eq, Ord, PartialEq, PartialOrd};
use core::convert::TryInto;
use core::fmt;
use core::hash::Hash;

use rkyv::{Archive as RkyvArchive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

/// Cryptography version fourcc code
pub type CryptoKind = FourCC;

#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    RkyvArchive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(repr(C), derive(CheckBytes))]
pub struct TypedKey {
    pub kind: CryptoKind,
    pub key: PublicKey,
}

impl TypedKey {
    pub fn new(kind: CryptoKind, key: PublicKey) -> Self {
        Self { kind, key }
    }
}

impl fmt::Display for TypedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}:{}", self.kind, self.key.encode())
    }
}
impl FromStr for TypedKey {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != (5 + PUBLIC_KEY_LENGTH_ENCODED) || b[4..5] != b":"[..] {
            apibail_parse_error!("invalid typed key", s);
        }
        let kind: CryptoKind = b[0..4].try_into().expect("should not fail to convert");
        let key = PublicKey::try_decode_bytes(&b[5..])?;
        Ok(Self { kind, key })
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    RkyvArchive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(repr(C), derive(CheckBytes))]
pub struct TypedKeyPair {
    pub kind: CryptoKind,
    pub key: PublicKey,
    pub secret: SecretKey,
}

impl TypedKeyPair {
    pub fn new(kind: CryptoKind, key: PublicKey, secret: SecretKey) -> Self {
        Self { kind, key, secret }
    }
}

impl fmt::Display for TypedKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}:{}:{}",
            self.kind,
            self.key.encode(),
            self.secret.encode()
        )
    }
}
impl FromStr for TypedKeyPair {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != (5 + PUBLIC_KEY_LENGTH_ENCODED + 1 + SECRET_KEY_LENGTH_ENCODED)
            || b[4..5] != b":"[..]
            || b[5 + PUBLIC_KEY_LENGTH_ENCODED..6 + PUBLIC_KEY_LENGTH_ENCODED] != b":"[..]
        {
            apibail_parse_error!("invalid typed key pair", s);
        }
        let kind: CryptoKind = b[0..4].try_into().expect("should not fail to convert");
        let key = PublicKey::try_decode_bytes(&b[5..5 + PUBLIC_KEY_LENGTH_ENCODED])?;
        let secret = SecretKey::try_decode_bytes(&b[5 + PUBLIC_KEY_LENGTH_ENCODED + 1..])?;
        Ok(Self { kind, key, secret })
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    RkyvArchive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(repr(C), derive(CheckBytes))]
pub struct TypedSignature {
    pub kind: CryptoKind,
    pub signature: Signature,
}
impl TypedSignature {
    pub fn new(kind: CryptoKind, signature: Signature) -> Self {
        Self { kind, signature }
    }
    pub fn from_keyed(tks: &TypedKeySignature) -> Self {
        Self {
            kind: tks.kind,
            signature: tks.signature,
        }
    }
    pub fn from_pair_sig(tkp: &TypedKeyPair, sig: Signature) -> Self {
        Self {
            kind: tkp.kind,
            signature: sig,
        }
    }
}

impl fmt::Display for TypedSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}:{}", self.kind, self.signature.encode())
    }
}
impl FromStr for TypedSignature {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != (5 + SIGNATURE_LENGTH_ENCODED) || b[4..5] != b":"[..] {
            apibail_parse_error!("invalid typed signature", s);
        }
        let kind: CryptoKind = b[0..4].try_into()?;
        let signature = Signature::try_decode_bytes(&b[5..])?;
        Ok(Self { kind, signature })
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    RkyvArchive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(repr(C), derive(CheckBytes))]
pub struct TypedKeySignature {
    pub kind: CryptoKind,
    pub key: PublicKey,
    pub signature: Signature,
}

impl TypedKeySignature {
    pub fn new(kind: CryptoKind, key: PublicKey, signature: Signature) -> Self {
        Self {
            kind,
            key,
            signature,
        }
    }
    pub fn as_typed_signature(&self) -> TypedSignature {
        TypedSignature {
            kind: self.kind,
            signature: self.signature,
        }
    }
}

impl fmt::Display for TypedKeySignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}:{}:{}",
            self.kind,
            self.key.encode(),
            self.signature.encode()
        )
    }
}
impl FromStr for TypedKeySignature {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != (5 + PUBLIC_KEY_LENGTH_ENCODED + 1 + SIGNATURE_LENGTH_ENCODED)
            || b[4] != b':'
            || b[5 + PUBLIC_KEY_LENGTH_ENCODED] != b':'
        {
            apibail_parse_error!("invalid typed key signature", s);
        }
        let kind: CryptoKind = b[0..4].try_into().expect("should not fail to convert");
        let key = PublicKey::try_decode_bytes(&b[5..5 + PUBLIC_KEY_LENGTH_ENCODED])?;
        let signature = Signature::try_decode_bytes(&b[5 + PUBLIC_KEY_LENGTH_ENCODED + 1..])?;
        Ok(Self {
            kind,
            key,
            signature,
        })
    }
}
