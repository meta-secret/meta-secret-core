use super::{
    repo::{Fault, Point, Repo},
    support::*,
    *,
};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

struct CodecSaved {
    state: State,
    witness: LocalGuardedPreparationObject,
    index: LocalGuardedPreparationIndexObject,
}
async fn codec_close<R: KvLogEventRepo>(inner: Arc<R>) -> Result<CodecSaved> {
    let h = H::new(inner).await?;
    // Codec fixture: actual encrypted input containers, no private K1/Accepted claim.
    let mut witness = super::codec::codec_record(&h).await?;
    h.p.repo.save(witness.clone()).await?;
    witness.0.value.payload = None;
    witness.0.value.phase = GuardedPreparationPhase::RetiredUnqueued;
    h.p.repo.replace(witness.clone()).await?;
    let mut index = LocalGuardedPreparationIndexObject(KvLogEvent {
        key: KvKey::from(LocalGuardedPreparationIndexDescriptor {
            owner: h.state.b.user_id(),
        }),
        value: LocalGuardedPreparationIndex {
            version: 1,
            owner: h.state.b.user_id(),
            request_ids: vec![h.state.uuid.clone()],
            revision: 0,
        },
    });
    h.p.repo.save(index.clone()).await?;
    index.0.value.revision = 1;
    h.p.repo.replace(index.clone()).await?;
    let H { p, state, wire } = h;
    drop(wire);
    drop(p);
    Ok(CodecSaved {
        state,
        witness,
        index,
    })
}
async fn codec_open<R: KvLogEventRepo>(inner: Arc<R>, saved: CodecSaved) -> Result<()> {
    ensure!(
        inner.find_one(saved.state.witness_id()).await? == Some(saved.witness.to_generic()),
        "actual file witness serde/scrub roundtrip failed"
    );
    ensure!(
        inner.find_one(saved.state.index_id()).await? == Some(saved.index.to_generic()),
        "actual file index serde/revision roundtrip failed"
    );
    ensure!(
        inner
            .find_one(saved.state.witness_id().next())
            .await?
            .is_none(),
        "codec replace left payload history"
    );
    drop(inner);
    Ok(())
}

struct Saved {
    state: State,
    keys: Vec<ArtifactId>,
    index: LocalGuardedPreparationIndexObject,
    body: Option<GenericKvLogEvent>,
    payload: Option<GuardedPreparationPayload>,
}
async fn interrupt<R: KvLogEventRepo>(inner: Arc<R>, queued: bool) -> Result<Saved> {
    let h = H::new(inner).await?;
    h.p.repo.arm(
        if queued {
            Point::Queued
        } else {
            Point::Witness
        },
        Fault::Before,
    );
    let r = h.decide().await;
    eprintln!(
        "C1b file interrupt queued={queued} actual_hits={}",
        h.p.repo.hits.load(Ordering::SeqCst)
    );
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "file interruption boundary UNREACHED"
    );
    let (body, payload) = if queued {
        let receipt = r?;
        ensure!(receipt.phase==meta_secret_core::node::common::model::liveboard::LiveboardOperationPhase::Unknown,"actual body gap lost Unknown");
        (
            Some(
                h.p.repo
                    .inner
                    .find_one(receipt.event_id)
                    .await?
                    .context("actual body")?,
            ),
            h.witness().await?.0.value.payload,
        )
    } else {
        ensure!(r.is_err(), "prequeue interruption reported receipt");
        h.no_body_or_send().await?;
        (None, None)
    };
    let index = h.index().await?;
    let keys = h.p.repo.snapshot().await?.into_keys().collect();
    let H { p, state, wire } = h;
    drop(wire);
    drop(p); // ALL actual local backend handles are released here.
    Ok(Saved {
        state,
        keys,
        index,
        body,
        payload,
    })
}
async fn reopen<R: KvLogEventRepo>(inner: Arc<R>, saved: Saved) -> Result<()> {
    let repo = Arc::new(Repo::new(
        inner,
        saved.state.b.user_id(),
        saved.state.uuid.clone(),
    ));
    repo.remember(saved.keys);
    let h = H::open(repo, saved.state)?;
    ensure!(
        h.index().await? == saved.index,
        "actual file reopen changed index codec"
    );
    if saved.body.is_none() {
        ensure!(
            h.p.repo
                .inner
                .find_one(h.state.witness_id())
                .await?
                .is_none(),
            "index-only crash became payload/queued after reopen"
        );
        h.p.repo.arm(Point::Index, Fault::Drop);
        let r = h.decide().await;
        ensure!(
            h.p.repo.hits.load(Ordering::SeqCst) > 0,
            "sameUUID reopen bypassed changed revision"
        );
        ensure!(r.is_err(), "dropped changed revision accepted");
        h.no_body_or_send().await?;
        h.p.repo.clear();
    }
    let r = h.decide().await?;
    let witness = h.witness().await?;
    super::stage::private_crypto(&h.state, &witness)?;
    if let Some(body) = saved.body {
        ensure!(
            r.event_id == body.obj_id()
                && h.p.repo.inner.find_one(r.event_id).await? == Some(body)
                && witness.0.value.payload == saved.payload,
            "reopen allocated/reencrypted actual body or payload"
        );
    } else {
        ensure!(
            h.index().await?.0.value.revision > saved.index.0.value.revision,
            "index retry did not advance changed revision"
        );
    }
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id().next())
            .await?
            .is_none(),
        "file backend grew private history"
    );
    Ok(())
}
#[tokio::test]
async fn s11_sqlite_real_close_reopen_index_committed_without_payload_requires_changed_probe()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let codec_url = format!("file:{}", dir.path().join("c1b-codec.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: codec_url.clone(),
    }
    .migrate();
    let codec = codec_close(Arc::new(SqlIteRepo {
        conn_url: codec_url.clone(),
    }))
    .await?;
    codec_open(
        Arc::new(SqlIteRepo {
            conn_url: codec_url,
        }),
        codec,
    )
    .await?;
    eprintln!("C1b SQLite actual codec close/reopen PASS; no business lifecycle inferred");
    let url = format!("file:{}", dir.path().join("c1b.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let saved = interrupt(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        false,
    )
    .await?;
    reopen(Arc::new(SqlIteRepo { conn_url: url }), saved).await
}
#[tokio::test]
async fn s12_redb_real_close_reopen_body_phase_gap_reuses_exact_actual_source_and_cipher()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let codec_path = dir.path().join("c1b-codec.redb");
    let codec = codec_close(Arc::new(ReDbRepo::new(&codec_path)?)).await?;
    codec_open(Arc::new(ReDbRepo::open(&codec_path)?), codec).await?;
    eprintln!("C1b ReDB actual codec close/reopen PASS; no business lifecycle inferred");
    let path = dir.path().join("c1b.redb");
    let saved = interrupt(Arc::new(ReDbRepo::new(&path)?), true).await?;
    reopen(Arc::new(ReDbRepo::open(&path)?), saved).await
}
