//! Bounded, payload-free validation shared by exact-decision client boundaries.
use crate::node::api::{CanonicalDecisionInput, is_liveboard_request_id};
use anyhow::{Result, anyhow, ensure};

impl CanonicalDecisionInput {
    pub fn parse_json(json: &str) -> Result<Self> {
        ensure!(json.len() <= 1024, "InvalidDecisionInput");
        let input: Self =
            serde_json::from_str(json).map_err(|_| anyhow!("InvalidDecisionInput"))?;
        ensure!(
            is_liveboard_request_id(&input.request_id),
            "InvalidDecisionInput"
        );
        Ok(input)
    }
}
