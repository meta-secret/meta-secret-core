use super::*;
use meta_secret_core::node::{
    common::model::secret::SsDistributionId,
    db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
};
fn expected(state: &State, pass: &MetaPasswordId) -> Result<&'static str> {
    if *pass == state.passes[0] {
        Ok("C1_PRIVATE_FIXTURE_FIRST")
    } else {
        ensure!(*pass == state.passes[1], "foreign fixture Pass");
        Ok("C1_PRIVATE_FIXTURE_SECOND")
    }
}
pub(super) async fn decline<R: KvLogEventRepo>(
    case: &ActualCase<R>,
    w: &LocalGuardedPreparationObject,
    p: &GuardedPreparationPayload,
) -> Result<()> {
    ensure!(
        p.passes.is_empty()
            && w.0.value.claims.is_empty()
            && w.0.value.packets.is_empty()
            && p.update.decision == RemoveMemberDecision::Declined
            && p.update.request.share_transfers.len() == 2
            && p.update.share_transfers.is_empty(),
        "real Decline must be COMPLETE zero-output and contain both encrypted inputs"
    );
    let mut passes = HashSet::new();
    for transfer in &p.update.request.share_transfers {
        ensure!(
            passes.insert(transfer.pass_id.clone())
                && transfer.receiver == *case.h.state.b.device_id(),
            "actual Decline request input set/receiver differs"
        );
        let id = ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: transfer.pass_id.clone(),
            receiver: case.h.state.b.device_id().clone(),
        }));
        let local = case
            .h
            .p
            .repo
            .find_one(id)
            .await?
            .context("actual zero-output request input")?
            .shared_secret()?
            .to_distribution_data()?;
        ensure!(
            transfer.claim_id.as_ref() == Some(&local.claim_id.id),
            "request transfer input generation differs"
        );
        let own = local
            .secret_message
            .cipher_text()
            .decrypt(&case.h.state.b.device_creds.secret_box.transport.sk)?;
        let transferred: EncryptedMessage = serde_json::from_str(&transfer.encrypted_share)?;
        let transferred = transferred
            .cipher_text()
            .decrypt(&case.h.state.b.device_creds.secret_box.transport.sk)?;
        let own = UserShareDto::try_from(&own.msg)?;
        let transferred = UserShareDto::try_from(&transferred.msg)?;
        ensure!(
            own.share_blocks
                .iter()
                .all(|b| b.config.threshold == 2 && b.config.number_of_shares == 3)
                && recover_from_shares(vec![own, transferred])?.text
                    == expected(&case.h.state, &transfer.pass_id)?,
            "real Decline encrypted input failed actual K2 recovery"
        );
    }
    ensure!(
        passes == case.h.state.passes.iter().cloned().collect(),
        "real Decline omitted an original sensitive transfer"
    );
    Ok(())
}
pub(super) async fn accepted<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    w: &LocalGuardedPreparationObject,
) -> Result<()> {
    let payload =
        w.0.value
            .payload
            .as_ref()
            .context("actual current accepted FULL")?;
    ensure!(
        payload.passes.len() == 1
            && payload.passes[0].pass_id == h.case.h.state.passes[1]
            && payload.update.decision == RemoveMemberDecision::Accepted
            && payload.update.request.share_transfers.len() == 1
            && payload.update.request.share_transfers[0].pass_id == h.case.h.state.passes[1]
            && payload
                .update
                .share_transfers
                .iter()
                .all(|t| t.pass_id == h.case.h.state.passes[1]),
        "complete current preparation is not p1-only"
    );
    let pass = &payload.passes[0];
    let input = h
        .physical
        .find_one(pass.input.distribution_id.clone())
        .await?
        .context("actual current p1 input")?
        .shared_secret()?;
    ensure!(
        digest(&input)? == pass.input.distribution_commitment
            && input.clone().to_distribution_data()?.claim_id == pass.input.claim_id
            && pass.claim.sender == *h.case.h.state.b.device_id()
            && pass.claim.receivers == vec![h.case.h.state.a.device_id().clone()],
        "p1 frozen input/generation/claim binding differs"
    );
    ensure!(
        pass.distributions.len() == 2,
        "p1 preparation needs both real surviving outputs"
    );
    let mut receivers = HashSet::new();
    for packet in &pass.distributions {
        let data = packet.clone().to_distribution_data()?;
        ensure!(
            data.claim_id == pass.claim.dist_claim_id && data.claim_id.pass_id == pass.pass_id,
            "p1 output generation differs"
        );
        let receiver = data
            .secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id();
        let user = if receiver == *h.case.h.state.a.device_id() {
            &h.case.h.state.a
        } else {
            ensure!(
                receiver == *h.case.h.state.b.device_id(),
                "removed/foreign peer got frozen output"
            );
            &h.case.h.state.b
        };
        ensure!(
            receivers.insert(receiver),
            "duplicate actual survivor output"
        );
        k1(data.secret_message, user)?;
    }
    Ok(())
}
fn k1(message: EncryptedMessage, user: &UserCreds) -> Result<()> {
    let plain = message
        .cipher_text()
        .decrypt(&user.device_creds.secret_box.transport.sk)?;
    let share = UserShareDto::try_from(&plain.msg)?;
    ensure!(
        !share.share_blocks.is_empty()
            && share
                .share_blocks
                .iter()
                .all(|b| b.config.threshold == 1 && b.config.number_of_shares == 2)
            && recover_from_shares(vec![share])?.text == "C1_PRIVATE_FIXTURE_SECOND",
        "actual surviving K1 cannot recover p1"
    );
    Ok(())
}
pub(super) async fn published<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    frozen: &GuardedPreparationPayload,
) -> Result<()> {
    for packet in &frozen.passes[0].distributions {
        let intended = packet.clone().to_distribution_data()?;
        let receiver = intended
            .secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id();
        let (actual, user) = if receiver == *h.case.h.state.b.device_id() {
            (
                h.physical.find_one(packet.obj_id()).await?,
                &h.case.h.state.b,
            )
        } else {
            ensure!(receiver == *h.case.h.state.a.device_id(), "foreign output");
            (
                h.case.h.state.server_repo.find_one(packet.obj_id()).await?,
                &h.case.h.state.a,
            )
        };
        let actual = actual.context("actual durable published survivor material")?;
        ensure!(
            actual == packet.clone().to_generic(),
            "actual publication differs from frozen bytes"
        );
        k1(
            actual
                .shared_secret()?
                .to_distribution_data()?
                .secret_message,
            user,
        )?;
    }
    Ok(())
}
