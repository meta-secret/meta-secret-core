use super::*;

fn packet_id(pass: &MetaPasswordId, receiver: &DeviceId) -> ArtifactId {
    ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: receiver.clone(),
    }))
}
pub(super) async fn packet<R: KvLogEventRepo>(
    client: &client::Client<R>,
    pass: &MetaPasswordId,
    receiver: &DeviceId,
) -> Result<Option<SsWorkflowObject>> {
    client
        .p
        .repo
        .find_one(packet_id(pass, receiver))
        .await?
        .map(|e| e.shared_secret())
        .transpose()
}
pub(super) async fn remote_packet(
    repo: &Arc<InMemKvLogEventRepo>,
    pass: &MetaPasswordId,
    receiver: &DeviceId,
) -> Result<Option<SsWorkflowObject>> {
    repo.find_one(packet_id(pass, receiver))
        .await?
        .map(|e| e.shared_secret())
        .transpose()
}
pub(super) async fn active_claim<R: KvLogEventRepo>(
    client: &client::Client<R>,
    packet: &SsWorkflowObject,
) -> Result<SsClaim> {
    let data = packet.clone().to_distribution_data()?;
    PersistentSharedSecret::from(client.p.clone())
        .get_ss_log_obj(client.creds.vault_name.clone())
        .await?
        .claims
        .get(&data.claim_id.id)
        .cloned()
        .context("actual current canonical Split claim")
}
pub(super) async fn own_tail<R: KvLogEventRepo>(
    client: &client::Client<R>,
) -> Result<SsDeviceLogObject> {
    PersistentSharedSecret::from(client.p.clone())
        .find_ss_device_log_tail_event(client.creds.device_id().clone())
        .await?
        .context("actual own Split metadata tail")
}
pub(super) fn share(packet: &SsWorkflowObject, creds: &UserCreds) -> Result<UserShareDto> {
    let data = packet.clone().to_distribution_data()?;
    let plain = data
        .secret_message
        .cipher_text()
        .decrypt(&creds.device_creds.secret_box.transport.sk)?;
    Ok(UserShareDto::try_from(&plain.msg)?)
}
pub(super) fn config(share: &UserShareDto, k: usize, n: usize) -> Result<()> {
    ensure!(
        !share.share_blocks.is_empty()
            && share
                .share_blocks
                .iter()
                .all(|b| b.config.threshold == k && b.config.number_of_shares == n),
        "actual share threshold/member configuration differs"
    );
    Ok(())
}
pub(super) fn received_packet(
    events: &[GenericKvLogEvent],
    pass: &MetaPasswordId,
    receiver: &DeviceId,
) -> Option<SsWorkflowObject> {
    events
        .iter()
        .find(|e| e.obj_id() == packet_id(pass, receiver))
        .and_then(|e| e.clone().shared_secret().ok())
}
