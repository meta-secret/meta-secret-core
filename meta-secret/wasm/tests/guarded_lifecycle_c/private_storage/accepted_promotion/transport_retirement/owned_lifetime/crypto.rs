use super::*;

async fn packet(peer: &peers::Peer, pass: &MetaPasswordId) -> Result<SsWorkflowObject> {
    peer.p
        .repo
        .find_one(packet_id(pass, &peer.creds))
        .await?
        .context("actual addressed peer delivery absent")?
        .shared_secret()
}
fn share(packet: &SsWorkflowObject, who: &UserCreds) -> Result<UserShareDto> {
    let raw = packet.clone().to_distribution_data()?;
    ensure!(
        raw.secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id()
            == *who.device_id(),
        "actual wrong recipient"
    );
    let plain = raw
        .secret_message
        .cipher_text()
        .decrypt(&who.device_creds.secret_box.transport.sk)?;
    let share = UserShareDto::try_from(&plain.msg)?;
    ensure!(
        !share.share_blocks.is_empty()
            && share
                .share_blocks
                .iter()
                .all(|b| b.config.threshold == 2 && b.config.number_of_shares == 3),
        "actual K2 is not threshold2/n3"
    );
    Ok(share)
}
pub(super) async fn all_pairs(
    h: &harness::Harness,
    a: &peers::Peer,
    c: &peers::Peer,
    pass: &MetaPasswordId,
    b: &SsWorkflowObject,
) -> Result<()> {
    let pa = packet(a, pass).await?;
    let pc = packet(c, pass).await?;
    let claim = b.clone().to_distribution_data()?.claim_id;
    let b_data = b.clone().to_distribution_data()?;
    ensure!(
        b_data.vault_name == h.case.h.state.b.vault_name
            && b_data.claim_id.pass_id == *pass
            && b_data
                .secret_message
                .cipher_text()
                .channel
                .sender()
                .to_device_id()
                == *h.case.h.state.b.device_id(),
        "actual B share Vault/Pass/local sender differs"
    );
    ensure!(
        [&pa, &pc].iter().all(
            |p| (*p)
                .clone()
                .to_distribution_data()
                .is_ok_and(|d| d.claim_id == claim
                    && d.vault_name == h.case.h.state.b.vault_name
                    && d.claim_id.pass_id == *pass
                    && d.secret_message
                        .cipher_text()
                        .channel
                        .sender()
                        .to_device_id()
                        == *h.case.h.state.b.device_id())
        ),
        "real Join mixed owner/generation"
    );
    let a = share(&pa, &a.creds)?;
    let b = share(b, &h.case.h.state.b)?;
    let c = share(&pc, &c.creds)?;
    for pair in [
        vec![a.clone(), b.clone()],
        vec![a.clone(), c.clone()],
        vec![b.clone(), c.clone()],
    ] {
        ensure!(
            recover_from_shares(pair)?.text == "C1_PRIVATE_FIXTURE_SECOND",
            "actual K2 pair recovery differs"
        );
    }
    for single in [a, b, c] {
        ensure!(
            recover_from_shares(vec![single]).is_err(),
            "singleton recovered threshold2 Secret"
        );
    }
    Ok(())
}
