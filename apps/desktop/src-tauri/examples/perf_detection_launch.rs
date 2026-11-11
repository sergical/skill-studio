#![allow(clippy::expect_used, clippy::print_stdout)]

use std::path::PathBuf;
use std::time::Instant;

#[allow(clippy::assertions_on_constants)]
fn main() {
    assert!(!cfg!(debug_assertions), "use a release build");
    let home = PathBuf::from(std::env::var_os("SKILL_STUDIO_FIXTURE").expect("fixture home"));
    assert!(home.join(".skill-studio-perf-fixture").is_file());
    assert_eq!(
        dirs::home_dir()
            .expect("HOME")
            .canonicalize()
            .expect("home path"),
        home.canonicalize().expect("fixture path")
    );
    let bin = PathBuf::from(std::env::var_os("SKILL_STUDIO_PERF_BIN").expect("fixture binaries"));
    for name in ["claude", "codex", "opencode", "pi", "cursor-agent", "grok"] {
        assert!(bin.join(name).is_file(), "missing fixture binary {name}");
    }
    let started = Instant::now();
    let report = tauri::async_runtime::block_on(
        skill_studio_lib::skills::harness_first_run::detect_with_runtime(
            skill_studio_lib::skills::core_runtime::build_runtime_detect,
        ),
    )
    .expect("launch detection");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert!(report
        .harnesses
        .iter()
        .all(|row| row.version.value.is_some()
            && row
                .executable
                .as_ref()
                .is_some_and(|path| path.starts_with(&bin))));
    println!(
        "{}",
        serde_json::json!({"elapsed_ms":elapsed_ms,"agents":report.harnesses.len()})
    );
}
