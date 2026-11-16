//! Thin native JSON producer ports with parse-before-manager error priority.
use crate::producer_json_runtime::{invoke, parse};

pub fn delete_secret_checked(secret_id: String) -> String {
    invoke(|manager| async move { manager.delete_secret_checked(&secret_id).await })
}
pub fn remove_member_checked(target_device_id: String) -> String {
    invoke(|manager| async move { manager.remove_member_checked(&target_device_id).await })
}
pub fn reconcile_producer_receipt(reference_json: String) -> String {
    let reference = match parse(&reference_json) {
        Ok(value) => value,
        Err(error) => return error,
    };
    invoke(|manager| async move { manager.reconcile_producer_receipt(reference).await })
}
pub fn continue_local_remove_checked(reference_json: String) -> String {
    let reference = match parse(&reference_json) {
        Ok(value) => value,
        Err(error) => return error,
    };
    invoke(|manager| async move { manager.continue_local_remove_checked(reference).await })
}
