use serde::{Deserialize, Serialize};

use crate::models::{
    FindSharesResult, MembershipStatus, MetaPasswordsData, PasswordRecoveryRequest, RegistrationStatus,
    SecretDistributionDocData
};
use crate::node::db::models::VaultInfo;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MessageType {
    Ok,
    Err,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericMessage<T> {
    pub msg_type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<ErrorMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMessage {
    stacktrace: Vec<String>,
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

impl<T> GenericMessage<T> {
    pub fn just_ok() -> Self {
        GenericMessage {
            msg_type: MessageType::Ok,
            data: None,
            err: None,
        }
    }

    pub fn data(data: T) -> Self {
        GenericMessage {
            msg_type: MessageType::Ok,
            data: Some(data),
            err: None,
        }
    }

    pub fn err(err: anyhow::Error) -> Self {
        let err_msg = ErrorMessage::from(&err);

        GenericMessage {
            msg_type: MessageType::Err,
            data: None,
            err: Some(err_msg),
        }
    }

    pub fn std_err(err: &dyn std::error::Error) -> Self {
        let err_msg = ErrorMessage::from(err);

        GenericMessage {
            msg_type: MessageType::Err,
            data: None,
            err: Some(err_msg),
        }
    }
}

pub type SecretDistributionDocResponse = GenericMessage<SecretDistributionDocData>;

pub type RegistrationResponse = GenericMessage<RegistrationStatus>;

pub type VaultInfoResponse = GenericMessage<VaultInfo>;

pub type MetaPasswordsResponse = GenericMessage<MetaPasswordsData>;

pub type PasswordRecoveryClaimsResponse = GenericMessage<Vec<PasswordRecoveryRequest>>;

pub type UserSharesResponse = GenericMessage<FindSharesResult>;

pub mod basic {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MongoDbStats {
        pub connection: bool,
        pub registrations: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HttpStatusDetails {
        pub http_status: String,
        pub http_status_code: u16,
        pub uri: String,
        pub method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub content_type: Option<String>,
    }
}

pub type MembershipResponse = GenericMessage<MembershipStatus>;

#[cfg(test)]
mod test {
    use crate::models::MembershipStatus;
    use anyhow::anyhow;
    use serde::{Deserialize, Serialize};
    use serde_json::error::Result;
    use thiserror::__private::AsDynError;

    use crate::sdk::api::{ErrorMessage, GenericMessage, MembershipResponse};

    #[test]
    fn test_generic_message() -> Result<()> {
        let msg: GenericMessage<String> = GenericMessage::just_ok();
        let msg = serde_json::to_string(&msg)?;
        assert_eq!(r#"{"msgType":"ok"}"#.to_string(), msg);

        let msg = MembershipResponse::data(MembershipStatus::AlreadyMember);
        let msg = serde_json::to_string(&msg)?;
        assert_eq!(r#"{"msgType":"ok","data":"AlreadyMember"}"#, msg);

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TestTest {
            pub xxx: String,
        }

        let msg = GenericMessage::data(TestTest { xxx: "yay".to_string() });
        let msg = serde_json::to_string(&msg)?;
        assert_eq!(r#"{"msgType":"ok","data":{"xxx":"yay"}}"#, msg);

        Ok(())
    }

    #[test]
    fn error_message_test() -> Result<()> {
        let err_msg = ErrorMessage::from(anyhow!("yay").context("my root cause").as_dyn_error());
        assert_eq!(r#"{"stacktrace":["my root cause"]}"#, serde_json::to_string(&err_msg)?);

        Ok(())
    }
}
