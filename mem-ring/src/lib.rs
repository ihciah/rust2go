// Copyright 2024 ihciah. All Rights Reserved.

// The runtime branches are selected with `all(feature = "tokio", not(feature = "monoio"))`
// gates, so enabling both features would silently select the monoio internals
// while dependents observe tokio as enabled. Reject the combination instead.
#[cfg(all(feature = "monoio", feature = "tokio"))]
compile_error!("features `monoio` and `tokio` are mutually exclusive; enable exactly one");

mod eventfd;
mod queue;
mod util;

pub use queue::{Guard, PushJoinHandle, Queue, QueueMeta, ReadQueue, WriteQueue};
