use super::*;

impl Scenario {
    pub async fn new() -> Result<Self> {
        let f = FixtureRegistry::base().await?.state.empty;
        for (p, creds, master) in [
            (
                f.p_obj.client.clone(),
                f.user_creds.client.clone(),
                f.device_creds.client_master_key.clone(),
            ),
            (
                f.p_obj.client_b.clone(),
                f.user_creds.client_b.clone(),
                f.device_creds.client_b_master_key.clone(),
            ),
            (
                f.p_obj.vd.clone(),
                f.user_creds.vd.clone(),
                f.device_creds.vd_master_key.clone(),
            ),
        ] {
            let credentials = PersistentCredentials {
                p_obj: p,
                master_key: master,
            };
            credentials
                .save_device_creds(creds.device_creds.clone())
                .await?;
            credentials.save_user_creds(creds).await?;
        }
        let mut vault = VaultObject::sign_up(
            f.user_creds.client.vault_name.clone(),
            member(&f.user_creds.client),
        );
        vault.0.value = vault
            .0
            .value
            .update_membership(UserMembership::Member(member(&f.user_creds.client_b)))
            .update_membership(UserMembership::Member(member(&f.user_creds.vd)));
        // Canonical registered baseline only. All subsequent changes use signed Core ports.
        f.p_obj.server.repo.save(vault).await?;
        f.p_obj
            .server
            .repo
            .save(VaultLogObject::create(member(&f.user_creds.client)))
            .await?;
        let server = Arc::new(ServerApp::new(
            f.p_obj.server.repo.clone(),
            f.device_creds.server_master_key,
        )?);
        server.init().await?;
        let wire = Arc::new(ObservedWire {
            inner: SyncProtocolFixture::new(server).sync_protocol,
            actions: Mutex::new(vec![]),
        });
        let a = Actor::new(
            f.p_obj.client,
            f.user_creds.client,
            f.device_creds.client_master_key,
            wire.clone(),
        );
        let b = Actor::new(
            f.p_obj.client_b,
            f.user_creds.client_b,
            f.device_creds.client_b_master_key,
            wire.clone(),
        );
        let c = Actor::new(
            f.p_obj.vd,
            f.user_creds.vd,
            f.device_creds.vd_master_key,
            wire.clone(),
        );
        let input = PlainPassInfo::new("same-sender-pending".into(), VALUE.into());
        let pass = input.pass_id.clone();
        let receipt = a.service.cluster_distribution_checked(input).await?;
        assert_eq!(receipt.kind, LiveboardOperationKind::Create);
        assert_eq!(receipt.secret_id.as_deref(), Some(pass.id_str().as_str()));
        assert!(
            receipt.error_code.is_none(),
            "real initial Secret creation must succeed"
        );
        let h = Self {
            a,
            b,
            c,
            pass,
            wire,
            canonical: f.p_obj.server,
        };
        // Initial distribution and its delivery ACKs precede the tested pending Recover.
        for _ in 0..2 {
            for actor in [&h.a, &h.b, &h.c] {
                actor.member().await?;
            }
        }
        for actor in [&h.a, &h.b, &h.c] {
            let info = actor.member().await?;
            assert_eq!(info.member.vault.members().len(), 3);
            let state = info.liveboard_state.context("real Liveboard metadata")?;
            assert_eq!(state.storage_mode, "shares");
            assert!(
                state
                    .secrets
                    .iter()
                    .any(|s| s.secret_id == h.pass.id_str() && s.readiness == "ready")
            );
        }
        let a_share = local_share(&h.a.service.p_obj, &h.pass, &h.a.creds).await?;
        let b_share = local_share(&h.b.service.p_obj, &h.pass, &h.b.creds).await?;
        assert_eq!(a_share.share_blocks[0].config.threshold, 2);
        assert!(meta_secret_core::recover_from_shares(vec![a_share.clone()]).is_err());
        assert!(meta_secret_core::recover_from_shares(vec![a_share, b_share])?.text == VALUE);
        Ok(h)
    }
}
