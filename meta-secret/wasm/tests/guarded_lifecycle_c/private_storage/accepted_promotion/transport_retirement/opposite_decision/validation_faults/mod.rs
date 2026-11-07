//! Five malformed-history cases and nine actual changed rollover interruptions.
mod checks;
mod faults;
mod fixture;
mod history;
mod observer;
mod sqlite;

use super::*;
use meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;
use meta_secret_core::node::{
    common::model::{liveboard::LiveboardOperationPhase, secret::SsDistributionId},
    db::{
        descriptors::shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
        events::local_guarded_preparation::GuardedCompactAttempt,
    },
};

type OldHarness<R> = super::harness::Harness<R>;
type OldRepo<R> = super::repo::Outer<R>;
type OldWire<R> = super::wire::Forward<R>;
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cut {
    ComputeRead,
    IndexDrop,
    FullBefore,
    FullDrop,
    FullReadback,
    FullLostReply,
    OriginReadback,
    BodyBefore,
    BodyReadback,
}
#[derive(Clone)]
enum Operation {
    Read(ArtifactId),
    Key(ArtifactId),
    Write(bool, Box<GenericKvLogEvent>),
    Delete(ArtifactId),
    TryDelete(ArtifactId),
    Clean,
    Recovery,
}
#[derive(Clone)]
struct Call {
    order: usize,
    operation: Operation,
}
#[derive(Clone)]
struct Success {
    order: usize,
    event: GenericKvLogEvent,
}
