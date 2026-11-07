use super::*;
#[derive(Clone, Copy, Debug)]
enum Negative {
    PriorTarget,
    PriorUuid,
    DuplicateFullId,
    CrossCurrentTechnical,
}
#[tokio::test]
async fn four_actual_queued_opposite_history_bindings_fail_closed_before_append() -> Result<()> {
    let mut results = vec![];
    for mode in [
        Negative::PriorTarget,
        Negative::PriorUuid,
        Negative::DuplicateFullId,
        Negative::CrossCurrentTechnical,
    ] {
        results.push(run(mode).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn run(mode: Negative) -> Result<()> {
    eprintln!("ROLLOVER HISTORY {mode:?} START actual healthy queued N+1");
    let (e, queued) = fixture::queued(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = fixture::server_hash(&e.h).await?;
    let mut mutant = e.h.case.h.witness().await?;
    let current = mutant
        .0
        .value
        .origin
        .as_ref()
        .context("actual N+1")?
        .event_id
        .clone();
    ensure!(
        current == queued.event_id && mutant.0.value.previous_attempts.len() == 1,
        "real prior/current fixture missing"
    );
    match mode {
        Negative::PriorTarget => {
            mutant.0.value.previous_attempts[0].origin.target_id =
                e.h.case.h.state.a.device_id().clone()
        }
        Negative::PriorUuid => {
            let wrong = "7a31fd8c-183d-4fd1-8e72-672cb1860b34";
            ensure!(
                wrong != e.h.case.h.state.uuid
                    && meta_secret_core::node::api::is_liveboard_request_id(wrong),
                "wrongUUID must remain syntax-valid"
            );
            mutant.0.value.previous_attempts[0].origin.request_id = wrong.into();
        }
        Negative::DuplicateFullId => mutant.0.value.previous_attempts[0].origin.event_id = current,
        Negative::CrossCurrentTechnical => {
            mutant.0.value.transport_noop =
                mutant.0.value.previous_attempts[0].transport_noop.clone()
        }
    }
    e.h.physical.replace(mutant.clone()).await?; // Explicit malformed LOCAL fixture, never canonical history.
    checks::reject(&e, &mutant, &server, &format!("{mode:?}")).await
}
