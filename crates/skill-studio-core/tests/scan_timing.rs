// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `scan`'s `OpTiming`: named steps, and the invariant that each step's
//! `elapsed_ms` never exceeds the whole call's `elapsed_ms`.

use std::sync::Arc;

use skill_studio_core::dto::ScanRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::ops::scan;
use skill_studio_core::ports::{OpContext, Ports, Runtime};
use skill_studio_core::testing::golden::{ctx, scope_for, unique_temp_dir};
use skill_studio_core::testing::{fixtures, FakeIds, NoHistory, RecordingSink};

use skill_studio_host::{FileLease, RealFs, SystemClock};

/// Runs a scan against the `basic` fixture in a fresh home and returns its
/// recorded timing.
fn basic_scan_timing(tag: &str) -> skill_studio_core::timing::OpTiming {
    let dir = unique_temp_dir(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.canonicalize().unwrap();

    let (_, builder) = fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == "basic")
        .expect("basic fixture registered");
    builder
        .materialize(&home)
        .unwrap_or_else(|e| panic!("materialize basic: {e}"));

    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(SystemClock::new()),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(NoHistory),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),

        telemetry: std::sync::Arc::new(skill_studio_core::ports::NoopTelemetry),

        scan_memo: None,
    };
    let scope = scope_for("basic", &home);
    let rt = Runtime::new(&scope, ports).expect("runtime");
    let op_ctx: OpContext = ctx();

    scan(&rt, &op_ctx, &ScanRequest::default()).expect("scan");
    op_ctx.take_timing().expect("scan must record a timing")
}

#[test]
fn scan_records_named_steps_within_the_op_elapsed_time() {
    let timing = basic_scan_timing("scan_timing_basic");

    assert_eq!(timing.op, "scan");

    for step in &timing.steps {
        assert!(
            step.elapsed_ms <= timing.elapsed_ms,
            "step {} took {}ms, more than the {}ms op total",
            step.name,
            step.elapsed_ms,
            timing.elapsed_ms
        );
    }
}

/// The per-folder work `roots_walk` covers most of - the folder walk, the
/// content hash, the token count and the scan-memo stamp - needs a step of
/// its own, or a slow scan has an unnamed majority.
#[test]
fn scan_names_the_per_folder_work_inside_roots_walk() {
    let timing = basic_scan_timing("scan_timing_folder_steps");

    let roots_walk = timing
        .steps
        .iter()
        .find(|s| s.name == "roots_walk")
        .expect("roots_walk step");

    for name in [
        "scan_memo_stamp",
        "folder_facts_walk",
        "folder_content_hash",
        "folder_token_count",
    ] {
        let step = timing
            .steps
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("step {name} missing from the scan timing"));
        assert_eq!(
            step.parent.as_deref(),
            Some("roots_walk"),
            "{name} must be a sub-step of roots_walk"
        );
        assert!(
            step.elapsed_ms <= roots_walk.elapsed_ms,
            "{name} took {}ms, more than the {}ms roots_walk covers",
            step.elapsed_ms,
            roots_walk.elapsed_ms
        );
    }
}
