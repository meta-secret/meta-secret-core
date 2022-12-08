use crate::shared_secret::shared_secret::RecoveryError;
use std::string::FromUtf8Error;

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("Invalid Base64 content")]
    InvalidBase64Content {
        #[from]
        source: base64::DecodeError,
    },

    #[error("SignatureError")]
    SignatureError {
        #[from]
        source: ed25519_dalek::SignatureError,
    },

    #[error("Invalid array size")]
    InvalidArraySize {
        #[from]
        source: std::array::TryFromSliceError,
    },

    #[error("Invalid utf8 array")]
    StringConversionError {
        #[from]
        source: FromUtf8Error,
    },

    #[error("Utf8 error")]
    Utf8ConversionError {
        #[from]
        source: std::str::Utf8Error,
    },

    #[error("Encryption error")]
    EncryptionError {
        #[from]
        source: crypto_box::aead::Error,
    },

    #[error("Json parsing error")]
    JsonParseError {
        #[from]
        source: serde_json::Error,
    },

    #[error("Password recovery error")]
    RecoveryError {
        #[from]
        source: RecoveryError,
    },

    #[error("unknown error")]
    Unknown,
}
