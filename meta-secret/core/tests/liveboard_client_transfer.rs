//! Local transport boundaries only; synthetic replies are not domain outcomes.
#[path = "liveboard_client_transfer/diagnostics.rs"]
mod diagnostics;
#[path = "liveboard_client_transfer/lifetime.rs"]
mod lifetime;
#[path = "liveboard_client_transfer/privacy.rs"]
mod privacy;
#[path = "liveboard_client_transfer/queue.rs"]
mod queue;

use meta_secret_core::node::app::meta_app::messaging::{
    GenericAppStateRequest as Q, GenericAppStateResponse as A,
};
use meta_secret_core::node::common::model::{
    liveboard::{LiveboardOperationKind, LiveboardOperationPhase, LiveboardOperationReceipt},
    meta_pass::PlainPassInfo,
    vault::vault::VaultName,
};

fn request(name: &str) -> Q {
    Q::ClusterDistributionChecked(PlainPassInfo::new(
        name.into(),
        "SYNTHETIC-PRIVATE-SECRET".into(),
    ))
}
fn fire(index: usize) -> Q {
    Q::GenerateUserCreds(VaultName::from(format!("local-{index}")))
}
fn receipt(id: &str) -> A {
    A::OperationReceipt(LiveboardOperationReceipt {
        kind: LiveboardOperationKind::Create,
        secret_id: Some(id.into()),
        claim_id: None,
        request_id: None,
        phase: LiveboardOperationPhase::Queued,
        error_code: None,
    })
}
fn assert_receipt(value: A, expected: &str) {
    assert!(
        matches!(value, A::OperationReceipt(r) if r.secret_id.as_deref() == Some(expected)),
        "private reply reached a different caller"
    );
}
