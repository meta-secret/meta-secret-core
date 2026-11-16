use super::*;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        common::model::secret::{SecretDistributionType, SsDistributionStatus},
        db::{
            descriptors::shared_secret_descriptor::SsLogDescriptor,
            events::shared_secret_event::SsLogObject,
            objects::persistent_shared_secret::PersistentSharedSecret,
        },
        security::canonical_json,
    },
};
fn hash(packet: &SsWorkflowObject) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(packet)?)?))
}
pub(super) async fn local_peers<R: KvLogEventRepo>(
    repo: Arc<R>,
    e: &Expected,
) -> Result<Vec<SsWorkflowObject>> {
    let mut found = vec![];
    for intended in peer_packets(e) {
        if let Some(raw) = repo.find_one(intended.obj_id()).await? {
            let actual = raw.shared_secret()?;
            ensure!(
                actual == intended,
                "local peer copy differs from original frozen bytes"
            );
            found.push(actual);
        }
    }
    Ok(found)
}
pub(super) async fn ordinary<R: KvLogEventRepo>(
    repo: Arc<R>,
    e: &Expected,
    w: &LocalGuardedPreparationObject,
) -> Result<()> {
    ensure!(
        w.0.value.claims.len() == 2,
        "actual TWO allocated claims required"
    );
    for pass in &e.frozen.passes {
        let c =
            w.0.value
                .claims
                .iter()
                .find(|x| x.pass_id == pass.pass_id)
                .context("actual intended ownership")?;
        let did = c
            .ss_device_event_id
            .clone()
            .context("NONEMPTY actual SsDeviceLog key")?;
        let lid = c
            .ss_log_event_id
            .clone()
            .context("NONEMPTY actual SsLog key")?;
        let device: SsDeviceLogObject = repo
            .find_one(did.clone())
            .await?
            .context("actual ordinary claim")?
            .try_into()?;
        let log: SsLogObject = repo
            .find_one(lid.clone())
            .await?
            .context("actual ordinary SsLog")?
            .try_into()?;
        let key = KvKey::from(SsLogDescriptor::from(e.owner.vault_name.clone()));
        let claim = log
            .0
            .value
            .claims
            .get(&pass.claim.id)
            .context("actual stable intended Split")?;
        let mut stable = claim.clone();
        stable.status = pass.claim.status.clone();
        stable.client_status = pass.claim.client_status.clone();
        ensure!(
            c.handed_off
                && c.claim == pass.claim
                && device.obj_id() == did
                && device.0.value == pass.claim
                && log.obj_id() == lid
                && log.obj_id().fqdn == key.obj_id.fqdn
                && log.key().obj_desc == key.obj_desc
                && stable == pass.claim,
            "ordinary actual typed claim/key/Vault/generation proof differs"
        );
        for packet in &pass.distributions {
            let compact =
                w.0.value
                    .packets
                    .iter()
                    .find(|p| p.event_id == packet.obj_id())
                    .context("actual compact packet")?;
            ensure!(
                compact.handed_off && compact.body_commitment == hash(packet)?,
                "actual compact handedOff/hash proof missing"
            );
            if compact.receiver_id == e.owner.device_id {
                ensure!(
                    repo.find_one(packet.obj_id()).await? == Some(packet.clone().to_generic()),
                    "actual owned frozen B packet changed"
                );
            } else {
                ensure!(
                    compact.receiver_id == e.requester,
                    "removed/unexpected recipient in ordinary outputs"
                );
            }
        }
    }
    Ok(())
}
async fn a_request(case: &Case) -> Result<DataEventsResponse> {
    case.h
        .state
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
            SsRequest {
                sender: case.h.state.a.user(),
                ss_log: ArtifactId::from(SsLogDescriptor::from(case.h.state.a.vault_name.clone())),
            },
        ))))
        .await?
        .to_data()
}
fn delivered_packets(data: &DataEventsResponse, e: &Expected) -> Vec<SsWorkflowObject> {
    data.0
        .iter()
        .filter_map(|x| match x {
            GenericKvLogEvent::SsWorkflow(p @ SsWorkflowObject::Distribution(_))
                if scoped_peer(e, p) =>
            {
                Some(p.clone())
            }
            _ => None,
        })
        .collect()
}
pub(super) async fn deliver_to_a(case: &Case, e: &Expected) -> Result<()> {
    let p = PersistentSharedSecret::from(Arc::new(PersistentObject::new(
        case.h.state.server_repo.clone(),
    )));
    let before = p.get_ss_log_obj(e.owner.vault_name.clone()).await?;
    let response = a_request(case).await?;
    let actual = delivered_packets(&response, e);
    let expected = peer_packets(e);
    ensure!(
        actual.len() == 2
            && actual
                .iter()
                .map(ObjIdExtractor::obj_id)
                .collect::<HashSet<_>>()
                .len()
                == 2
            && actual.iter().all(|x| expected.contains(x)),
        "actual A request did not deliver EXACT TWO frozen packets"
    );
    for packet in &expected {
        ensure!(
            case.h
                .state
                .server_repo
                .find_one(packet.obj_id())
                .await?
                .is_none(),
            "actual delivered A queue key retained"
        );
    }
    let after = p.get_ss_log_obj(e.owner.vault_name.clone()).await?;
    let mut new_associations = 0;
    for pass in &e.frozen.passes {
        let changed: Vec<_> = after
            .claims
            .values()
            .filter(|c| {
                c.distribution_type == SecretDistributionType::Split
                    && c.dist_claim_id.pass_id == pass.pass_id
                    && c.status.get(&e.requester) == Some(&SsDistributionStatus::Delivered)
                    && before.claims.get(&c.id).is_some_and(|b| {
                        b.status.get(&e.requester) != Some(&SsDistributionStatus::Delivered)
                    })
            })
            .collect();
        ensure!(
            changed.len() == 1,
            "actual A Delivered association absent/duplicated for physical Pass"
        );
        if changed[0].id == pass.claim.id {
            new_associations += 1;
        }
    }
    ensure!(
        delivered_packets(&a_request(case).await?, e).is_empty(),
        "second actual A request redelivered a peer packet"
    );
    eprintln!(
        "C2 peer actual A delivery exact_packets=2 physical_keys_absent=2 second_delivery=0 inspected_Delivered_associations=2 new_B_claim_associations={new_associations} first_visited_policy_preserved=true"
    );
    Ok(())
}
