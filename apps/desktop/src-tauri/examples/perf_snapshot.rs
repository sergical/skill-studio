#![allow(clippy::expect_used, clippy::print_stdout)]

use skill_studio_host::SkillInvocationIndex;
use skill_studio_lib::skill_refresh::{build_snapshot, BuildPaths};

fn main() {
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
    let mut count = 0;
    for _ in 0..10 {
        let (snapshot, _) = build_snapshot(
            &home,
            &mut SkillInvocationIndex::default(),
            BuildPaths::new(&cache, &runs, &updates),
            chrono::Utc::now(),
        );
        count = snapshot.skills.len();
        samples.push(serde_json::to_vec(&Some(snapshot)).expect("IPC JSON").len());
    }
    println!(
        "{}",
        serde_json::json!({"desktop_first_list_ipc_bytes": samples, "desktop_skills": count})
    );
}
