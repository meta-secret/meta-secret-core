use crate::shared_secret::data_block::common::DataBlockParserError;
use shamirsecretsharing::SSSError;
use std::io;
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

    #[error("Unsuccesful recovery operation")]
    RecoveryError {
        #[from]
        source: RecoveryError,
    },

    #[error("Data block parsing error")]
    DataBlockParserError {
        #[from]
        source: DataBlockParserError,
    },

    #[error("Shamir secret sharing operation error")]
    ShamirError {
        #[from]
        source: SSSError,
    },

    #[error("Split operation failed")]
    SplitOperationError {
        #[from]
        source: SplitError,
    },

    #[error(transparent)]
    SharesLoaderError {
        #[from]
        source: SharesLoaderError,
    },

    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("Empty input")]
    EmptyInput(String),
    #[error("Invalid share")]
    InvalidShare(String),

    #[error("Failed recover operation")]
    ShamirCombineSharesError {
        #[from]
        source: SSSError,
    },
    #[error("Non utf8 string")]
    DeserializationError {
        #[from]
        source: FromUtf8Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SplitError {
    #[error("Secrets directory can't be created")]
    SecretsDirectoryError {
        #[from]
        source: io::Error,
    },
    #[error("User secret share: invalid format (can't be serialized into json)")]
    UserShareJsonSerializationError {
        #[from]
        source: serde_json::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SharesLoaderError {
    #[error(transparent)]
    FileSystemError(#[from] io::Error),
    #[error(transparent)]
    DeserializationError(#[from] serde_json::error::Error),
}
