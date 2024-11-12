use std::io;
use std::string::FromUtf8Error;

use shamirsecretsharing::SSSError;

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::MetaPublicKey;
use crate::node::common::model::crypto::CommunicationChannel;
use crate::secret::data_block::common::DataBlockParserError;

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error(transparent)]
    InvalidBase64Content {
        #[from]
        source: base64::DecodeError,
    },

    #[error(transparent)]
    SignatureError {
        #[from]
        source: ed25519_dalek::SignatureError,
    },

    #[error(transparent)]
    InvalidArraySize {
        #[from]
        source: std::array::TryFromSliceError,
    },

    #[error(transparent)]
    StringConversionError {
        #[from]
        source: FromUtf8Error,
    },

    #[error(transparent)]
    Utf8ConversionError {
        #[from]
        source: std::str::Utf8Error,
    },

    #[error(transparent)]
    EncryptionError {
        #[from]
        source: crypto_box::aead::Error,
    },

    #[error("The key manager: {key_manager_pk:?} is not a component of the secure communication channel: {channel:?}")]
    ThirdPartyEncryptionError {
        key_manager_pk: MetaPublicKey,
        channel: CommunicationChannel,
    },

    #[error(transparent)]
    JsonParseError {
        #[from]
        source: serde_json::Error,
    },

    #[error(transparent)]
    RecoveryError {
        #[from]
        source: RecoveryError,
    },

    #[error(transparent)]
    DataBlockParserError {
        #[from]
        source: DataBlockParserError,
    },

    #[error(transparent)]
    ShamirError {
        #[from]
        source: SSSError,
    },

    #[error(transparent)]
    SplitOperationError {
        #[from]
        source: SplitError,
    },

    #[error(transparent)]
    SharesLoaderError {
        #[from]
        source: SharesLoaderError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("Empty input")]
    EmptyInput(String),
    #[error("Invalid share")]
    InvalidShare(String),

    #[error(transparent)]
    ShamirCombineSharesError {
        #[from]
        source: SSSError,
    },
    #[error(transparent)]
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

#[derive(Debug, thiserror::Error)]
pub enum CredentialsError {
    #[error("Credentials not found")]
    NotFoundError(String),
}
