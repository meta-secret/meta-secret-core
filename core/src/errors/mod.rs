use std::string::FromUtf8Error;

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("Invalid Base64 content")]
    InvalidBase64Content(#[from] base64::DecodeError),

    #[error("SignatureError")]
    SignatureError(#[from] ed25519_dalek::SignatureError),

    #[error("Invalid array size")]
    InvalidArraySize(#[from] std::array::TryFromSliceError),

    #[error("Invalid utf8 array")]
    StringConversionError(#[from] FromUtf8Error),

    #[error("unknown error")]
    Unknown,
}
