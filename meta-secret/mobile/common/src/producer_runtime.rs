//! Typed producer return shares the existing process runtime and FFI lock.
use super::{FFI_CALL_LOCK, MobileApplicationManager, RUNTIME};
use anyhow::{Result, anyhow};
use meta_secret_core::node::common::model::liveboard_producer::LiveboardProducerReceipt;
use std::future::Future;

impl MobileApplicationManager {
    pub(crate) fn run_producer<F: Future<Output = Result<LiveboardProducerReceipt>>>(
        future: F,
    ) -> Result<LiveboardProducerReceipt> {
        let _owner = FFI_CALL_LOCK
            .lock()
            .map_err(|_| anyhow!("OperationUnavailable"))?;
        RUNTIME.block_on(future)
    }
}
