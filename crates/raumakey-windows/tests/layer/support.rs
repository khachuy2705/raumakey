//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// `<pid>-<n>`, unique within this test process and across concurrently running test processes.
pub fn unique_suffix() -> String {
    let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}
