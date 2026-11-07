use super::*;
pub(super) struct Current {
    pub pass: MetaPasswordId,
    pub a: SsWorkflowObject,
    pub b: SsWorkflowObject,
    pub c: SsWorkflowObject,
    pub claim: SsClaimId,
}
pub(super) fn packet_id(pass: &MetaPasswordId, who: &UserCreds) -> ArtifactId {
    ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: who.device_id().clone(),
    }))
}
pub(super) async fn capture(case: &Case, physical: &InMemKvLogEventRepo) -> Result<Current> {
    let pass = case.h.state.passes[1].clone();
    let mut values = vec![];
    for who in [&case.h.state.a, &case.h.state.b, &case.h.state.c] {
        let event = physical
            .find_one(packet_id(&pass, who))
            .await?
            .context("actual initial current K2 packet absent")?
            .shared_secret()?;
        let d = event.clone().to_distribution_data()?;
        ensure!(
            d.vault_name == case.h.state.b.vault_name
                && d.claim_id.pass_id == pass
                && d.secret_message
                    .cipher_text()
                    .channel
                    .receiver()
                    .to_device_id()
                    == *who.device_id()
                && d.secret_message
                    .cipher_text()
                    .channel
                    .sender()
                    .to_device_id()
                    == *case.h.state.a.device_id(),
            "initial current channel/recipient/Pass differs"
        );
        values.push(event);
    }
    let c = values.pop().unwrap();
    let b = values.pop().unwrap();
    let a = values.pop().unwrap();
    let claim = b.clone().to_distribution_data()?.claim_id;
    ensure!(
        a.clone().to_distribution_data()?.claim_id == claim
            && c.clone().to_distribution_data()?.claim_id == claim,
        "initial fixture mixed K2 generations"
    );
    let x = Current {
        pass,
        a,
        b,
        c,
        claim,
    };
    pairs(case, &x, &x.b, "initial_fixture")?;
    Ok(x)
}
fn share(packet: &SsWorkflowObject, who: &UserCreds) -> Result<UserShareDto> {
    let d = packet.clone().to_distribution_data()?;
    ensure!(
        d.secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id()
            == *who.device_id(),
        "actual packet receiver differs"
    );
    let p = d
        .secret_message
        .cipher_text()
        .decrypt(&who.device_creds.secret_box.transport.sk)?;
    let s = UserShareDto::try_from(&p.msg)?;
    ensure!(
        !s.share_blocks.is_empty()
            && s.share_blocks
                .iter()
                .all(|b| b.config.threshold == 2 && b.config.number_of_shares == 3),
        "actual current K2 config is not threshold2/n3"
    );
    Ok(s)
}
pub(super) fn pairs(
    case: &Case,
    current: &Current,
    actual_b: &SsWorkflowObject,
    phase: &str,
) -> Result<()> {
    ensure!(
        *actual_b == current.b
            && actual_b.clone().to_distribution_data()?.claim_id == current.claim,
        "actual imported B is not exact original current generation"
    );
    let a = share(&current.a, &case.h.state.a)?;
    let b = share(actual_b, &case.h.state.b)?;
    let c = share(&current.c, &case.h.state.c)?;
    for pair in [
        vec![a.clone(), b.clone()],
        vec![a.clone(), c.clone()],
        vec![b.clone(), c.clone()],
    ] {
        ensure!(
            recover_from_shares(pair)?.text == "C1_PRIVATE_FIXTURE_SECOND",
            "real current pair did not recover exact known Secret"
        );
    }
    for single in [a, b, c] {
        ensure!(
            recover_from_shares(vec![single]).is_err(),
            "one actual current share unexpectedly recovered"
        );
    }
    eprintln!(
        "C4_INCOMING phase={phase} actual_K2_3pairs_recover=true actual_3singletons_Err=true threshold2_n3=true"
    );
    Ok(())
}
