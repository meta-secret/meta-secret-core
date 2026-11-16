use super::{fault_repo::Repo, fixtures::*, protocol::Network};
use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    app::{meta_app::meta_client_service::MetaClientService, sync::sync_gateway::SyncGateway},
    common::model::{
        ApplicationState, VaultFullInfo,
        crypto::aead::EncryptedMessage,
        meta_pass::MetaPasswordId,
        secret::{ClaimId, SecretDistributionData, SsClaim, SsDistributionStatus},
        user::common::UserDataMember,
    },
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::{
            generic_log_event::*,
            kv_log_event::*,
            shared_secret_event::SsWorkflowObject,
            vault::liveboard_metadata::{DecisionOutcome, RecoveryDecisionRecord},
        },
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_shared_secret::PersistentSharedSecret,
        repo::generic_db::*,
    },
};
use meta_secret_core::secret::shared_secret::PlainText;
use meta_server_node::server::server_app::ServerApp;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

struct Accepted {
    seed: Harness,
    pass: MetaPasswordId,
    claim: SsClaim,
    service: Arc<MetaClientService<Repo, Network>>,
    mobile: mobile_common::app_manager::ApplicationManager<Repo, Network>,
    network: Arc<Network>,
}

impl Accepted {
    async fn new() -> Result<Self> {
        let mut seed = Harness::new(3).await?;
        let pass = seed.seed_secret("r32-source-bound-recovery").await?;
        let mut claim = seed
            .vault
            .clone()
            .to_vault_member(UserDataMember::from(seed.approver.user()))?
            .create_recovery_claim(pass.clone());
        claim.status.statuses.insert(
            seed.initiator.device_id().clone(),
            SsDistributionStatus::Sent,
        );
        claim.status.statuses.insert(
            seed.target.device_id().clone(),
            SsDistributionStatus::Declined,
        );
        let local = seed.share(&pass, &seed.approver).await?;
        let remote = seed.share(&pass, &seed.initiator).await?;
        ensure!(
            seed.vault.members().len() == 3,
            "fixture requires three actual members"
        );
        ensure!(
            local.share_id != remote.share_id,
            "fixture requires distinct actual shares"
        );
        for share in [&local, &remote] {
            ensure!(!share.share_blocks.is_empty(), "fixture share blocks exist");
            ensure!(
                share.share_blocks.iter().all(|block| {
                    block.config.threshold == 2 && block.config.number_of_shares == 3
                }),
                "fixture requires actual n3/k2 material"
            );
        }
        let encrypted = seed
            .initiator
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from(remote.as_json()?),
                &seed.approver.device().keys.transport_pk,
            )?;
        let recovery_id = claim
            .recovery_db_ids()
            .into_iter()
            .find(|id| id.distribution_id.receiver == *seed.initiator.device_id())
            .context("exact approved receiver source")?;
        seed.p
            .repo
            .save(SsWorkflowObject::Recovery(KvLogEvent {
                key: KvKey::from(SsWorkflowDescriptor::Recovery(recovery_id)),
                value: SecretDistributionData {
                    vault_name: seed.vault.vault_name.clone(),
                    claim_id: claim.dist_claim_id.clone(),
                    secret_message: EncryptedMessage::CipherShare { share: encrypted },
                },
            }))
            .await?;
        let ss = PersistentSharedSecret::from(seed.p.clone());
        ss.save_ss_log_event(claim.clone()).await?;
        let mut log = ss.get_ss_log_obj(seed.vault.vault_name.clone()).await?;
        log.liveboard_recovery_results.insert(
            claim.id.clone(),
            RecoveryDecisionRecord {
                claim_id: claim.id.clone(),
                responder_id: seed.initiator.device_id().clone(),
                outcome: DecisionOutcome::Accepted,
                source: None,
            },
        );
        seed.p
            .repo
            .save(
                ss.create_new_ss_log_object(log, seed.vault.vault_name.clone())
                    .await?,
            )
            .await?;

        let server_repo = Arc::new(InMemKvLogEventRepo::default());
        for value in seed.p.repo.get_db().await.into_values() {
            if !matches!(
                value,
                GenericKvLogEvent::UserCreds(_) | GenericKvLogEvent::DeviceCreds(_)
            ) {
                server_repo.save(value).await?;
            }
        }
        let server = Arc::new(ServerApp::new(server_repo.clone(), seed.master.clone())?);
        server.init().await?;
        let network = Arc::new(Network::new(server, server_repo));
        let gateway = Arc::new(SyncGateway::new(
            "r32-actual-recovery".into(),
            seed.p.clone(),
            network.clone(),
            seed.master.clone(),
        ));
        let original = seed.service();
        let service = Arc::new(MetaClientService {
            data_transfer: original.data_transfer,
            sync_gateway: gateway.clone(),
            state_provider: original.state_provider,
            p_obj: seed.p.clone(),
            device_data: seed.approver.device(),
            master_key: seed.master.clone(),
        });
        let mobile = mobile_common::app_manager::ApplicationManager::new(
            network.clone(),
            gateway,
            service.clone(),
            seed.master.clone(),
        );
        Ok(Self {
            seed,
            pass,
            claim,
            service,
            mobile,
            network,
        })
    }

    async fn projection(&self) -> Result<Value> {
        let ApplicationState::Vault(VaultFullInfo::Member(member)) =
            self.service.get_app_state().await?
        else {
            anyhow::bail!("actual current Member required");
        };
        let json = serde_json::to_value(member)?;
        ensure!(
            !json.to_string().contains(VALUE),
            "projection must exclude plaintext"
        );
        json.get("liveboardState")
            .cloned()
            .context("real serialized Liveboard projection")
    }

    async fn complete(&self) -> Result<()> {
        // The ordinary mobile method takes the existing client-operation guard,
        // reconstructs, signs completion, calls real ServerApp, then synchronizes.
        let run = self.service.run();
        tokio::select! {
            result = self.mobile.show_recovered(self.claim.id.clone()) => {
                let value = result?;
                ensure!(value.text == VALUE, "legacy exact recovered plaintext preserved");
            }
            result = run => { result?; anyhow::bail!("service ended before mobile recovery"); }
            _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("recovery watchdog elapsed"),
        }
        let server = PersistentSharedSecret::from(Arc::new(
            meta_secret_core::node::db::objects::persistent_object::PersistentObject::new(
                self.network.repo.clone(),
            ),
        ))
        .get_ss_log_obj(self.seed.vault.vault_name.clone())
        .await?;
        let terminal = server
            .claims
            .get(&self.claim.id)
            .context("actual server completion claim")?;
        ensure!(
            terminal.status.statuses.get(self.seed.approver.device_id())
                == Some(&SsDistributionStatus::Delivered),
            "actual signed completion must persist requester Delivered on server"
        );
        ensure!(
            terminal
                .status
                .statuses
                .get(self.seed.initiator.device_id())
                == Some(&SsDistributionStatus::Sent),
            "approved receiver remains Sent after actual completion"
        );
        Ok(())
    }
}

fn operation<'a>(state: &'a Value, claim: &ClaimId) -> Result<&'a Value> {
    state["operations"]
        .as_array()
        .context("operation list")?
        .iter()
        .find(|op| op["requestId"] == claim.wasm_id_str())
        .context("exact Recover operation")
}

fn secret<'a>(state: &'a Value, pass: &MetaPasswordId) -> Result<&'a Value> {
    state["secrets"]
        .as_array()
        .context("secret list")?
        .iter()
        .find(|item| item["secretId"] == pass.id_str())
        .context("exact current Secret")
}

async fn transition() -> Result<(Value, Value, ClaimId)> {
    let h = Accepted::new().await?;
    let before = h.projection().await?;
    ensure!(
        secret(&before, &h.pass)?["primaryAction"] == "showRecovered",
        "eligible real Accepted action"
    );
    ensure!(
        secret(&before, &h.pass)?["readiness"] == "ready",
        "eligible actual local+approved remote material"
    );
    ensure!(
        operation(&before, &h.claim.id)?["clientStatus"] == "accepted",
        "actual Accepted operation"
    );
    let ss = PersistentSharedSecret::from(h.seed.p.clone());
    let sources = ss.get_recoveries(h.claim.clone()).await?;
    ensure!(
        sources.len() == 1,
        "one exact actual approved Recovery source"
    );
    let local = h.seed.distribution(&h.pass, &h.seed.approver).await?;
    h.complete().await?;
    let after = h.projection().await?;
    ensure!(
        operation(&after, &h.claim.id)?["clientStatus"] == "done",
        "actual normal Done projection"
    );
    ensure!(
        secret(&after, &h.pass)?["primaryAction"] == "recover",
        "normal Core terminal action transition"
    );
    ensure!(
        secret(&after, &h.pass)?["recoveryClaimId"].is_null(),
        "Done is excluded from active selected claims"
    );
    ensure!(
        ss.get_recoveries(h.claim.clone()).await? == sources,
        "normal requester sync retains exact Recovery ciphertext source"
    );
    ensure!(
        h.seed.distribution(&h.pass, &h.seed.approver).await? == local,
        "normal completion retains local Distribution ciphertext"
    );
    Ok((before, after, h.claim.id))
}

#[tokio::test]
async fn legacy_mobile_recovery_preserves_value_and_reaches_real_server_done() -> Result<()> {
    transition().await?;
    Ok(())
}

#[tokio::test]
async fn accepted_and_actual_done_expose_equal_source_bound_reveal_context() -> Result<()> {
    // All positive fixture/reconstruction/server/sync preconditions run before
    // this behavioral RED. No future type/import/compiler failure is the oracle.
    let (before, after, claim) = transition().await?;
    let accepted = operation(&before, &claim)?
        .get("revealContext")
        .filter(|value| value.is_object())
        .context("RED: eligible Accepted operation must expose Core revealContext")?;
    let done = operation(&after, &claim)?
        .get("revealContext")
        .filter(|value| value.is_object())
        .context("RED: actual normal Done operation must preserve Core revealContext")?;
    ensure!(
        accepted == done,
        "captured source context must survive only normal completion"
    );
    ensure!(accepted["version"] == 1, "explicit context version");
    ensure!(
        accepted["recoverySources"]
            .as_array()
            .is_some_and(|sources| sources.len() == 1),
        "context binds actual consumed remote ciphertext source"
    );
    Ok(())
}
