//! Strong new public-manager checks; the original 19 binding assertions are unchanged.
use super::{fault_repo::Repo, fixtures::*, transport::Protocol};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    app::meta_app::meta_client_service::MetaClientService,
    common::model::{
        liveboard::{LiveboardOperationPhase, LiveboardOperationReceipt},
        secret::{SecretDistributionType, SsClaim},
    },
    db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::shared_secret_event::SsDeviceLogObject,
    },
};
use std::{future::Future, sync::Arc, time::Duration};

async fn execute<T>(
    service: &MetaClientService<Repo, Protocol>,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        result = operation => result,
        result = service.run() => { result?; anyhow::bail!("service exited before checked receipt") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("checked manager watchdog elapsed"),
    }
}
async fn strict_unknown(web: bool) -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("public-unconfirmed-recovery").await?;
    let service = Arc::new(h.service());
    let (first, second) = if web {
        let manager = meta_secret_wasm::app_manager::ApplicationManager::new(
            h.protocol.clone(),
            h.gateway(),
            service.clone(),
            h.master.clone(),
        );
        execute(&service, async {
            let first = manager.recover_checked(pass.clone()).await?;
            let second = manager.recover_checked(pass.clone()).await?;
            Ok((first, second))
        })
        .await?
    } else {
        let manager = mobile_common::app_manager::ApplicationManager::new(
            h.protocol.clone(),
            h.gateway(),
            service.clone(),
            h.master.clone(),
        );
        execute(&service, async {
            let first = manager.recover_checked(pass.clone()).await?;
            let second = manager.recover_checked(pass.clone()).await?;
            Ok((first, second))
        })
        .await?
    };
    let claims: Vec<SsClaim> =
        h.p.get_object_events_from_beginning(SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ))
        .await?
        .into_iter()
        .map(|e: SsDeviceLogObject| e.to_distribution_request())
        .filter(|c| c.distribution_type == SecretDistributionType::Recover)
        .collect();
    assert_eq!(
        claims.len(),
        1,
        "manager retry must not allocate another Claim"
    );
    for receipt in [first, second] {
        assert_eq!(
            receipt.claim_id.as_deref(),
            Some(claims[0].id.wasm_id_str().as_str())
        );
        assert_eq!(receipt.secret_id.as_deref(), Some(pass.id_str().as_str()));
        assert_eq!(receipt.phase, LiveboardOperationPhase::Unknown);
        assert_eq!(
            receipt.error_code.as_deref(),
            Some("RecoverCanonicalUnknown")
        );
        let json = serde_json::to_value(&receipt)?;
        assert!(
            json.as_object()
                .context("public manager receipt")?
                .keys()
                .all(|key| [
                    "kind",
                    "secretId",
                    "claimId",
                    "requestId",
                    "phase",
                    "errorCode"
                ]
                .contains(&key.as_str()))
        );
        assert_eq!(json["phase"], "unknown");
        assert_eq!(json["errorCode"], "RecoverCanonicalUnknown");
        for private in [
            VALUE,
            "PRIVATE-ERROR-CONTENT",
            "secretMessage",
            "shareBlocks",
            "signature",
            "transportSk",
        ] {
            assert!(!json.to_string().contains(private));
        }
        let roundtrip: LiveboardOperationReceipt = serde_json::from_value(json)?;
        assert_eq!(roundtrip, receipt);
    }
    Ok(())
}

#[tokio::test]
async fn web_manager_serializes_same_actual_unknown_recovery_with_fixed_canonical_code()
-> Result<()> {
    strict_unknown(true).await
}
#[tokio::test]
async fn mobile_manager_serializes_same_actual_unknown_recovery_with_fixed_canonical_code()
-> Result<()> {
    strict_unknown(false).await
}
