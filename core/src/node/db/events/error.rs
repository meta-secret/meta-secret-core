use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::shared_secret_event::{SSDeviceLogObject, SSLedgerObject, SharedSecretObject};
use crate::node::db::events::vault_event::VaultActionEvent;
use thiserror::Error;
use crate::node::db::events::vault::device_log_event::DeviceLogObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMessage {
    stacktrace: Vec<String>,
}

#[derive(Error, Debug)]
pub enum LogEventCastError {
    #[error("InvalidGlobalIndex: Invalid event")]
    InvalidGlobalIndex(GenericKvLogEvent),
    #[error("InvalidCredentials: Invalid event")]
    InvalidCredentials(GenericKvLogEvent),
    #[error("InvalidDbTail: Invalid event")]
    InvalidDbTail(GenericKvLogEvent),
    #[error("InvalidDeviceLog: Invalid event")]
    InvalidDeviceLog(GenericKvLogEvent),
    #[error("InvalidVaultLog: Invalid event")]
    InvalidVaultLog(GenericKvLogEvent),
    #[error("InvalidVault: Invalid event")]
    InvalidVault(GenericKvLogEvent),
    #[error("InvalidVaultMembership: Invalid event")]
    InvalidVaultMembership(GenericKvLogEvent),
    #[error("InvalidSharedSecret: Invalid event")]
    InvalidSharedSecret(GenericKvLogEvent),
    #[error("InvalidSSDeviceLog: Invalid event")]
    InvalidSSDeviceLog(GenericKvLogEvent),
    #[error("InvalidSSLedger: Invalid event")]
    InvalidSSLedger(GenericKvLogEvent),
    #[error("WrongSSLedger: wrong event")]
    WrongSSLedger(SSLedgerObject),
    #[error("WrongSSDeviceLog: wrong event")]
    WrongSSDeviceLog(SSDeviceLogObject),
    #[error("WrongSharedSecret: wrong event")]
    WrongSharedSecret(SharedSecretObject),
    #[error("WrongDeviceLog: wrong event")]
    WrongDeviceLog(DeviceLogObject),
    #[error("WrongVaultAction. Expected: {0}, actual: {1}")]
    WrongVaultAction(String, VaultActionEvent),
    
}

impl From<&anyhow::Error> for ErrorMessage {
    fn from(err: &anyhow::Error) -> Self {
        let mut stacktrace = vec![];
        for cause in err.chain() {
            stacktrace.push(cause.to_string().trim().to_string());
        }

        Self { stacktrace }
    }
}

impl From<&dyn std::error::Error> for ErrorMessage {
    fn from(err: &dyn std::error::Error) -> Self {
        let mut stacktrace = vec![];

        let mut current_error = err;
        while let Some(source) = current_error.source() {
            let err_msg = format!("{}", current_error);
            stacktrace.push(err_msg);

            current_error = source;
        }

        Self { stacktrace }
    }
}
