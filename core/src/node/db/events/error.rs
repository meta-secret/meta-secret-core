use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMessage {
    stacktrace: Vec<String>,
}

#[derive(Error, Debug)]
pub enum LogEventCastError {
    #[error("Invalid event")]
    InvalidEventType,
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
