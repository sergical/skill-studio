#![allow(clippy::expect_used, clippy::print_stdout)]

use skill_studio_host::SkillInvocationIndex;
use skill_studio_lib::skill_refresh::{build_snapshot, BuildPaths};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--perf-build-id") {
        println!(
            "{}",
            option_env!("SKILL_STUDIO_PERF_BUILD").unwrap_or("unmarked")
        );
        return;
    }
    let home = std::path::PathBuf::from(
        std::env::var_os("SKILL_STUDIO_FIXTURE").expect("fixture required"),
    );
    assert_eq!(
        std::env::var_os("HOME"),
        Some(home.clone().into_os_string())
    );
    let cache = home.join(".cache/perf-invocations.json");
    let runs = home.join(".data/runs");
    let updates = home.join(".data/updates.json");
    let mut samples = Vec::new();
    let mut outcomes = Vec::new();
    let sample_count: usize = std::env::args()
        .nth(1)
        .expect("sample count")
        .parse()
        .expect("runs");
    assert!(sample_count >= 10);
    for _ in 0..sample_count {
        let (snapshot, _) = build_snapshot(
            &home,
            &mut SkillInvocationIndex::default(),
            BuildPaths::new(&cache, &runs, &updates),
            chrono::Utc::now(),
        );
        outcomes.push(serde_json::json!({
            "skills": snapshot.skills.len(),
            "scan_partial": snapshot.scan_partial,
        }));
        samples.push(serde_json::to_vec(&Some(snapshot)).expect("IPC JSON").len());
    }
    println!(
        "{}",
        serde_json::json!({"desktop_first_list_ipc_bytes": samples, "desktop_scan": outcomes})
    );
}
