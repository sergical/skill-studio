#![allow(clippy::expect_used, clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use skill_studio_core::bench_estate::estate;
use skill_studio_core::dto::ScanRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::ops::scan;
use skill_studio_core::ports::Runtime;
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::ctx;
use skill_studio_host::default_ports;

fn runtime(home: &Path, projects: &[PathBuf]) -> Runtime {
    let mut scope = RuntimeScope::fixture(home);
    scope.read_timeout_ms = 60_000;
    scope.projects = ProjectSelection::Explicit {
        paths: projects.to_vec(),
    };
    Runtime::new(
        &scope,
        default_ports(
            home.join(".history/leases"),
            Arc::new(HarnessCatalog::builtin()),
        ),
    )
    .expect("fixture runtime")
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let home = PathBuf::from(args.get(1).expect("fixture output home"));
    let count: usize = args.get(2).expect("skill count").parse().expect("count");
    let runs: usize = args.get(3).expect("sample count").parse().expect("runs");
    assert!(runs >= 10);
    assert!(!home.exists(), "use a new fixture directory");
    std::fs::create_dir_all(&home).expect("create fixture");
    let home = home.canonicalize().expect("canonical home");
    let generated = estate(count - 2, 1);
    let mut builder = generated.builder;
    for (agent, manifest, name) in [
        ("claude", "claude", "perf-claude"),
        ("codex", "codex", "perf-codex"),
    ] {
        let root = format!(".{agent}/plugins/cache/perf/{name}/1.0.0");
        builder = builder
            .file(
                &format!("{root}/.{manifest}-plugin/plugin.json"),
                format!(r#"{{"name":"{name}","version":"1.0.0","skills":"./skills"}}"#).as_bytes(),
            )
            .file(
                &format!("{root}/skills/{name}/SKILL.md"),
                format!("---\nname: {name}\ndescription: Benchmark plugin\n---\nPlugin body.\n")
                    .as_bytes(),
            );
    }
    builder.materialize(&home).expect("materialize");
    // Keep names and paths deterministic while exercising invalid YAML parsing.
    fn break_three(dir: &Path, remaining: &mut usize) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("fixture directory")
            .map(|e| e.expect("entry"))
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if *remaining == 0 {
                break;
            }
            if entry.file_type().expect("type").is_dir() {
                break_three(&entry.path(), remaining);
            } else if entry.file_name() == "SKILL.md" {
                std::fs::write(
                    entry.path(),
                    "---\nname: [broken\ndescription: invalid\n---\nBody\n",
                )
                .expect("broken fixture");
                *remaining -= 1;
            }
        }
    }
    break_three(&home.join(".agents"), &mut 3);
    let projects: Vec<_> = generated
        .project_dirs
        .iter()
        .map(|p| home.join(p))
        .collect();
    let mut fresh = Vec::new();
    let mut warm = Vec::new();
    let rt = runtime(&home, &projects);
    let mut inventory = None;
    for _ in 0..runs {
        let cold_rt = runtime(&home, &projects);
        let start = Instant::now();
        let result = scan(&cold_rt, &ctx(), &ScanRequest::default()).expect("fresh scan");
        fresh.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(result.skills.len(), count, "fixture scan count");
        let start = Instant::now();
        inventory = Some(scan(&rt, &ctx(), &ScanRequest::default()).expect("warm scan"));
        warm.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    std::fs::write(
        home.join("inventory.json"),
        serde_json::to_vec(&inventory).expect("inventory JSON"),
    )
    .expect("inventory output");
    println!(
        "{}",
        serde_json::json!({"projects": projects, "skills": count, "global_skills": generated.stats.global_count + 2, "seed": 1, "core_scan_fresh_runtime_ms": fresh, "core_scan_warm_runtime_ms": warm})
    );
}
