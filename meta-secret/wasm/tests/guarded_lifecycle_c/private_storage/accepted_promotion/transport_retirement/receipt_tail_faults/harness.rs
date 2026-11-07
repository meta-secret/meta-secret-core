use super::*;

pub(super) struct SeedProof {
    pub(super) reference: LocalGuardedPreparationObject,
    pub(super) body_hash: String,
    pub(super) index_hash: String,
    pub(super) registry_hash: String,
    pub(super) canonical_hash: String,
    pub(super) claim_identities: HashMap<String, String>,
}
pub(super) struct Harness<R: KvLogEventRepo> {
    pub case: GenericCase<R>,
    pub physical: Arc<R>,
    pub repo: Arc<repo::Outer<R>>,
    pub wire: Arc<wire::Forward<R>>,
    pub gateway: SyncGateway<repo::Outer<R>, wire::Forward<R>>,
    pub reference: LocalGuardedPreparationObject,
    pub body_hash: String,
    pub index_hash: String,
    pub registry_hash: String,
    pub canonical_hash: String,
    pub claim_identities: HashMap<String, String>,
}
impl<R: KvLogEventRepo> Harness<R> {
    pub async fn new(physical: Arc<R>, mode: Mode) -> Result<Self> {
        let case = GenericCase::new(physical.clone()).await?;
        let (receipt, frozen) = case.prepare(true).await?;
        let mut reference = case.h.witness().await?;
        private_crypto(&case.h.state, &reference)?;
        let origin = reference
            .0
            .value
            .origin
            .as_ref()
            .context("actual queued origin")?;
        let body = physical
            .find_one(receipt.event_id.clone())
            .await?
            .context("actual original N")?;
        ensure!(
            origin.event_id == receipt.event_id
                && frozen.passes.len() == 2
                && reference.0.value.payload.as_ref() == Some(&frozen)
                && origin.body_commitment
                    == producer_source_commitment(&body.clone().device_log()?)?
                && reference.0.value.transport_noop.is_none(),
            "real FULL2 queued N setup differs"
        );
        let body_hash = digest(&body)?;
        reference.0.value.payload = None; // Only compact expected metadata crosses a reopen.
        tombstone::delete_first(&case).await?;
        let index_hash = digest(&case.h.index().await?)?;
        let registry_hash = digest(
            &physical
                .find_one(ArtifactId::from(LocalRemovalEnvelopeDescriptor(
                    case.h.state.b.user_id(),
                )))
                .await?
                .context("actual indexed envelope registry")?,
        )?;
        let canonical_hash = digest(&canonical(&case).await?)?;
        let mut claim_identities = HashMap::new();
        for event in case.h.p.repo.snapshot().await?.into_values() {
            let claims = match event {
                GenericKvLogEvent::SsLog(log) => log.0.value.claims.into_values().collect(),
                GenericKvLogEvent::SsDeviceLog(log) => vec![log.0.value],
                _ => vec![],
            };
            for claim in claims {
                let key = claim.id.wasm_id_str();
                let identity = claim_identity(&claim)?;
                if let Some(previous) = claim_identities.insert(key, identity.clone()) {
                    ensure!(
                        previous == identity,
                        "actual baseline claim identity conflicts"
                    );
                }
            }
        }
        ensure!(
            reference
                .0
                .value
                .claims
                .iter()
                .all(|owned| !claim_identities.contains_key(&owned.claim.id.wasm_id_str())),
            "actual planned generation already published at trace baseline"
        );

        Self::from_case(
            case,
            physical,
            mode,
            SeedProof {
                reference,
                body_hash,
                index_hash,
                registry_hash,
                canonical_hash,
                claim_identities,
            },
        )
    }
    pub fn from_case(
        case: GenericCase<R>,
        physical: Arc<R>,
        mode: Mode,
        seed: SeedProof,
    ) -> Result<Self> {
        let SeedProof {
            reference,
            body_hash,
            index_hash,
            registry_hash,
            canonical_hash,
            claim_identities,
        } = seed;
        let origin = reference
            .0
            .value
            .origin
            .clone()
            .context("compact original origin")?;
        let repo = Arc::new(repo::Outer::new(&case, physical.clone(), origin, mode));
        let p = Arc::new(PersistentObject {
            repo: repo.clone(),
            device_log_append_guard: case.h.p.device_log_append_guard.clone(),
            client_operation_guard: case.h.p.client_operation_guard.clone(),
            device_sync_guard: case.h.p.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(
                &p.device_log_append_guard,
                &case.h.p.device_log_append_guard
            ) && Arc::ptr_eq(&p.client_operation_guard, &case.h.p.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &case.h.p.device_sync_guard),
            "outer observer changed SAME three guard Arcs"
        );
        let wire = Arc::new(wire::Forward::new(&case, repo.clone(), mode));
        let gateway = SyncGateway::new(
            "actual-receipt-tail-prefix".into(),
            p,
            wire.clone(),
            case.h.state.master.clone(),
        );
        Ok(Self {
            case,
            physical,
            repo,
            wire,
            gateway,
            reference,
            body_hash,
            index_hash,
            registry_hash,
            canonical_hash,
            claim_identities,
        })
    }
    pub async fn sync(&self) -> Result<()> {
        self.gateway.sync(self.case.h.state.b.user()).await
    }
    pub fn clear(&self) {
        self.repo.clear();
        self.wire.clear();
    }
}
