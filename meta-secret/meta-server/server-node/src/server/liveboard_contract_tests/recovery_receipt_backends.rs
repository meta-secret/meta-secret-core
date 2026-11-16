//! Each case closes the original real DB handle before reopening the same file.
use super::recovery_receipt_faults::{Fault, ReceiptFaultRepo, Step};
use super::recovery_receipt_fixture::ReceiptHarness;
use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::common::model::secret::SsClaim;
use std::sync::atomic::Ordering;

async fn interrupted<R: KvLogEventRepo>(
    inner: Arc<R>,
    accepted: bool,
    step: Step,
    fault: Fault,
) -> Result<(Harness, SsClaim, SignedAction)> {
    let repo = Arc::new(ReceiptFaultRepo::new(inner));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let packet = h.packet(&h.base.b, accepted)?;
    let server = h.server().await?;
    repo.arm(step, fault);
    assert!(
        send(&server, packet.clone()).await.is_err(),
        "RED: uncertain real backend persistence must fail closed"
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "RED: required real persistence stage was not reached"
    );
    Ok((h.base, h.claim, packet)) // All original backend/server/fault-wrapper Arcs drop here.
}

async fn reopened<R: KvLogEventRepo>(
    inner: Arc<R>,
    state: (Harness, SsClaim, SignedAction),
    accepted: bool,
    reserved: bool,
) -> Result<()> {
    let (base, claim, packet) = state;
    let h = ReceiptHarness {
        base,
        repo: inner,
        claim,
    };
    let server = h.server().await?;
    refresh(&server, &h.base.a).await?;
    if reserved {
        let first = h.assert_winner(accepted).await?;
        let _ = send(&server, h.packet(&h.base.c, !accepted)?).await;
        assert!(
            h.assert_winner(accepted).await? == first,
            "opposite receiver replaced durable reserved winner after file reopen"
        );
    }
    send(&server, packet)
        .await
        .context("RED: file reopen must allow only exact original source acknowledgement")?;
    h.assert_winner(accepted).await?;
    assert!(
        h.repo
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            .is_none()
    );
    Ok(())
}

async fn sqlite(accepted: bool, step: Step, fault: Fault) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("http-server.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let state = interrupted(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        accepted,
        step,
        fault,
    )
    .await?;
    reopened(
        Arc::new(SqlIteRepo { conn_url: url }),
        state,
        accepted,
        step != Step::Reserve || fault != Fault::Before,
    )
    .await
}
async fn redb(accepted: bool, step: Step, fault: Fault) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("server.redb");
    let state = interrupted(Arc::new(ReDbRepo::new(&path)?), accepted, step, fault).await?;
    reopened(
        Arc::new(ReDbRepo::open(&path)?),
        state,
        accepted,
        step != Step::Reserve || fault != Fault::Before,
    )
    .await
}
macro_rules! cases {
    ($approve:ident, $decline:ident, $backend:ident, $step:ident, $fault:ident) => {
        #[tokio::test]
        async fn $approve() -> Result<()> {
            $backend(true, Step::$step, Fault::$fault).await
        }
        #[tokio::test]
        async fn $decline() -> Result<()> {
            $backend(false, Step::$step, Fault::$fault).await
        }
    };
}
cases!(
    recovery_receipt_sqlite_approve_reserve_before,
    recovery_receipt_sqlite_decline_reserve_before,
    sqlite,
    Reserve,
    Before
);
cases!(
    recovery_receipt_sqlite_approve_reserve_lost_reply,
    recovery_receipt_sqlite_decline_reserve_lost_reply,
    sqlite,
    Reserve,
    LostReply
);
cases!(
    recovery_receipt_sqlite_approve_reserve_readback,
    recovery_receipt_sqlite_decline_reserve_readback,
    sqlite,
    Reserve,
    Readback
);
cases!(
    recovery_receipt_sqlite_approve_mark_before,
    recovery_receipt_sqlite_decline_mark_before,
    sqlite,
    Mark,
    Before
);
cases!(
    recovery_receipt_sqlite_approve_mark_lost_reply,
    recovery_receipt_sqlite_decline_mark_lost_reply,
    sqlite,
    Mark,
    LostReply
);
cases!(
    recovery_receipt_sqlite_approve_mark_readback,
    recovery_receipt_sqlite_decline_mark_readback,
    sqlite,
    Mark,
    Readback
);
cases!(
    recovery_receipt_sqlite_approve_delete_before,
    recovery_receipt_sqlite_decline_delete_before,
    sqlite,
    Remove,
    Before
);
cases!(
    recovery_receipt_sqlite_approve_delete_lost_reply,
    recovery_receipt_sqlite_decline_delete_lost_reply,
    sqlite,
    Remove,
    LostReply
);
cases!(
    recovery_receipt_sqlite_approve_delete_readback,
    recovery_receipt_sqlite_decline_delete_readback,
    sqlite,
    Remove,
    Readback
);
cases!(
    recovery_receipt_sqlite_approve_raw_before,
    recovery_receipt_sqlite_decline_raw_before,
    sqlite,
    Raw,
    Before
);
cases!(
    recovery_receipt_sqlite_approve_raw_lost_reply,
    recovery_receipt_sqlite_decline_raw_lost_reply,
    sqlite,
    Raw,
    LostReply
);
cases!(
    recovery_receipt_sqlite_approve_raw_readback,
    recovery_receipt_sqlite_decline_raw_readback,
    sqlite,
    Raw,
    Readback
);
cases!(
    recovery_receipt_sqlite_approve_canonical_before,
    recovery_receipt_sqlite_decline_canonical_before,
    sqlite,
    Canonical,
    Before
);
cases!(
    recovery_receipt_sqlite_approve_canonical_lost_reply,
    recovery_receipt_sqlite_decline_canonical_lost_reply,
    sqlite,
    Canonical,
    LostReply
);
cases!(
    recovery_receipt_sqlite_approve_canonical_readback,
    recovery_receipt_sqlite_decline_canonical_readback,
    sqlite,
    Canonical,
    Readback
);
cases!(
    recovery_receipt_redb_approve_reserve_before,
    recovery_receipt_redb_decline_reserve_before,
    redb,
    Reserve,
    Before
);
cases!(
    recovery_receipt_redb_approve_reserve_lost_reply,
    recovery_receipt_redb_decline_reserve_lost_reply,
    redb,
    Reserve,
    LostReply
);
cases!(
    recovery_receipt_redb_approve_reserve_readback,
    recovery_receipt_redb_decline_reserve_readback,
    redb,
    Reserve,
    Readback
);
cases!(
    recovery_receipt_redb_approve_mark_before,
    recovery_receipt_redb_decline_mark_before,
    redb,
    Mark,
    Before
);
cases!(
    recovery_receipt_redb_approve_mark_lost_reply,
    recovery_receipt_redb_decline_mark_lost_reply,
    redb,
    Mark,
    LostReply
);
cases!(
    recovery_receipt_redb_approve_mark_readback,
    recovery_receipt_redb_decline_mark_readback,
    redb,
    Mark,
    Readback
);
cases!(
    recovery_receipt_redb_approve_delete_before,
    recovery_receipt_redb_decline_delete_before,
    redb,
    Remove,
    Before
);
cases!(
    recovery_receipt_redb_approve_delete_lost_reply,
    recovery_receipt_redb_decline_delete_lost_reply,
    redb,
    Remove,
    LostReply
);
cases!(
    recovery_receipt_redb_approve_delete_readback,
    recovery_receipt_redb_decline_delete_readback,
    redb,
    Remove,
    Readback
);
cases!(
    recovery_receipt_redb_approve_raw_before,
    recovery_receipt_redb_decline_raw_before,
    redb,
    Raw,
    Before
);
cases!(
    recovery_receipt_redb_approve_raw_lost_reply,
    recovery_receipt_redb_decline_raw_lost_reply,
    redb,
    Raw,
    LostReply
);
cases!(
    recovery_receipt_redb_approve_raw_readback,
    recovery_receipt_redb_decline_raw_readback,
    redb,
    Raw,
    Readback
);
cases!(
    recovery_receipt_redb_approve_canonical_before,
    recovery_receipt_redb_decline_canonical_before,
    redb,
    Canonical,
    Before
);
cases!(
    recovery_receipt_redb_approve_canonical_lost_reply,
    recovery_receipt_redb_decline_canonical_lost_reply,
    redb,
    Canonical,
    LostReply
);
cases!(
    recovery_receipt_redb_approve_canonical_readback,
    recovery_receipt_redb_decline_canonical_readback,
    redb,
    Canonical,
    Readback
);
