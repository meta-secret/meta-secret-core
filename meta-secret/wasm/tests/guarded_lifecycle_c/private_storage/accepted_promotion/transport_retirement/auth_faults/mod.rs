//! Authorization uncertainty never excuses retaining sensitive private material.
mod harness;
mod lifecycle;
mod proof;
mod repo;
mod wire;

use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;
use std::collections::HashMap;

type ActualRepo = ExistingRepo<super::super::repo::Observed<local_repo::Repo>>;
type ActualWire = super::super::wire::Forward<local_repo::Repo>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Before,
    Drop,
    ReadbackErr,
    LostReply,
}
fn tick(clock: &AtomicUsize) -> usize {
    clock.fetch_add(1, Ordering::SeqCst) + 1
}

struct Write {
    order: usize,
    event: GenericKvLogEvent,
}
struct Read {
    order: usize,
    authorization: DeviceTransportNoop,
}
struct Durable {
    order: usize,
    authorization: DeviceTransportNoop,
}
