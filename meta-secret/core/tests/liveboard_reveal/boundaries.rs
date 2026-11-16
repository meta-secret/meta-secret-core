use super::fixture::Recovery;
use anyhow::{Result, ensure};
use meta_secret_core::{
    crypto::utils::Id48bit,
    node::{
        common::model::secret::ClaimId,
        db::{
            events::{generic_log_event::*, shared_secret_event::SsWorkflowObject},
            objects::{
                persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
            },
            repo::generic_db::*,
        },
    },
};

#[tokio::test]
async fn malformed_canonical_claim_map_key_fails_closed_without_panicking() -> Result<()> {
    let r = Recovery::new().await?;
    let ss = PersistentSharedSecret::from(r.h.p.clone());
    let mut log = ss.get_ss_log_obj(r.h.vault.vault_name.clone()).await?;
    let claim = log.claims.remove(&r.claim.id).unwrap();
    log.claims.insert(
        ClaimId::from(Id48bit::from("foreign-map-key".to_string())),
        claim,
    );
    let object = ss
        .create_new_ss_log_object(log, r.h.vault.vault_name.clone())
        .await?;
    r.h.p.repo.save(object).await?;
    ensure!(
        r.context().await.ok().flatten().is_none(),
        "RED: malformed map identity cannot issue usable proof"
    );
    Ok(())
}

#[tokio::test]
async fn actual_packet_subtype_descriptor_and_pass_mismatches_get_no_proof() -> Result<()> {
    for variant in 0..3 {
        let r = Recovery::new().await?;
        let mut packet = r.source().await?;
        if let SsWorkflowObject::Recovery(event) = &mut packet {
            match variant {
                0 => {
                    event.key.obj_desc = r.local().await?.key().obj_desc;
                }
                1 => {
                    event.value.claim_id.pass_id.name = "foreign-pass".into();
                }
                _ => {}
            }
        }
        if variant == 2
            && let SsWorkflowObject::Recovery(event) = packet
        {
            packet = SsWorkflowObject::Distribution(event);
        }
        r.h.p.repo.replace(packet).await?;
        ensure!(
            r.context().await.ok().flatten().is_none(),
            "wrong exact source subtype/descriptor/Pass cannot receive proof"
        );
    }
    Ok(())
}

#[tokio::test]
async fn membership_loss_and_tombstone_close_original_context_and_same_pass_cannot_revive()
-> Result<()> {
    for deletion in [false, true] {
        let r = Recovery::new().await?;
        let mut vault = PersistentVault::from(r.h.p.clone())
            .get_vault(r.h.vault.vault_name.clone())
            .await?;
        if deletion {
            vault.0.value = vault.0.value.delete_secret(r.pass.clone());
            let stale = vault.0.value.clone().add_secret(r.pass.clone());
            ensure!(
                !stale.secrets.contains(&r.pass),
                "existing same-Pass tombstone refusal remains authoritative"
            );
        } else {
            vault.0.value.users.remove(r.h.approver.device_id());
        }
        r.h.p.repo.replace(vault).await?;
        ensure!(
            r.context().await.ok().flatten().is_none(),
            "current membership/deletion cannot retain old disclosure context"
        );
    }
    Ok(())
}
