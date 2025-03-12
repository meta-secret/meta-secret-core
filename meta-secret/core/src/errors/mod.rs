use std::io;
use std::string::FromUtf8Error;

use crate::crypto::keys::TransportPk;
use crate::node::common::model::crypto::channel::CommunicationChannel;
use crate::secret::data_block::common::DataBlockParserError;
use shamirsecretsharing::SSSError;

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

    #[error("Invalid key size")]
    InvalidSizeEncryptionError { err_msg: String },

    #[error("The key manager: {key_manager_pk:?} is not a component of the secure communication channel: {channel:?}")]
    ThirdPartyEncryptionError {
        key_manager_pk: TransportPk,
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
    #[error("Communication channel error, device id not approved: {device:?}")]
    CommunicationChannelError { device: TransportPk },
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn test_recovery_error_empty_input() {
        let error = RecoveryError::EmptyInput("Test empty input".to_string());
        let error_message = format!("{}", error);
        assert!(error_message.contains("Empty input"));
    }
    
    #[test]
    fn test_recovery_error_invalid_share() {
        let error = RecoveryError::InvalidShare("Test invalid share".to_string());
        let error_message = format!("{}", error);
        assert!(error_message.contains("Invalid share"));
    }
    
    #[test]
    fn test_core_error_from_recovery_error() {
        let recovery_error = RecoveryError::EmptyInput("Test error".to_string());
        let core_error = CoreError::from(recovery_error);
        
        match core_error {
            CoreError::RecoveryError { source } => {
                match source {
                    RecoveryError::EmptyInput(msg) => assert_eq!(msg, "Test error"),
                    _ => panic!("Expected EmptyInput variant"),
                }
            },
            _ => panic!("Expected RecoveryError variant"),
        }
    }
    
    #[test]
    fn test_split_error_from_io_error() {
        let io_error = IoError::new(ErrorKind::NotFound, "Test IO error");
        let split_error = SplitError::from(io_error);
        
        match split_error {
            SplitError::SecretsDirectoryError { source } => {
                assert_eq!(source.kind(), ErrorKind::NotFound);
                assert_eq!(source.to_string(), "Test IO error");
            },
            SplitError::UserShareJsonSerializationError { .. } => {
                panic!("Expected SecretsDirectoryError variant, got UserShareJsonSerializationError");
            }
        }
    }
    
    #[test]
    fn test_shares_loader_error_from_io_error() {
        let io_error = IoError::new(ErrorKind::PermissionDenied, "Test permission error");
        let loader_error = SharesLoaderError::from(io_error);
        
        match loader_error {
            SharesLoaderError::FileSystemError(source) => {
                assert_eq!(source.kind(), ErrorKind::PermissionDenied);
                assert_eq!(source.to_string(), "Test permission error");
            },
            _ => panic!("Expected FileSystemError variant"),
        }
    }
    
    #[test]
    fn test_core_error_from_shares_loader_error() {
        let io_error = IoError::new(ErrorKind::NotFound, "Test file not found");
        let loader_error = SharesLoaderError::from(io_error);
        let core_error = CoreError::from(loader_error);
        
        match core_error {
            CoreError::SharesLoaderError { source } => {
                match source {
                    SharesLoaderError::FileSystemError(io_err) => {
                        assert_eq!(io_err.kind(), ErrorKind::NotFound);
                    },
                    _ => panic!("Expected FileSystemError variant"),
                }
            },
            _ => panic!("Expected SharesLoaderError variant"),
        }
    }
    
    #[test]
    fn test_credentials_error_not_found() {
        let error = CredentialsError::NotFoundError("missing_user".to_string());
        let error_string = format!("{}", error);
        
        assert!(error_string.contains("Credentials not found"));
    }
}
