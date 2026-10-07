// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
// The printed table is this test's output, so `print_stdout` is allowed too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

//! Timing check for the core read and write jobs on a 378-skill home, the
//! measured size of a real estate. Run on demand, never in CI:
//! `cargo test -p skill-studio-core --all-features --test timing -- --ignored --nocapture`.
//! It prints a table and asserts only correctness, never a duration.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use skill_studio_core::bench_estate::estate;
use skill_studio_core::dto::{Inventory, ParkRequest, RemoveRequest, ScanRequest, UnparkRequest};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{
    BackingRelationship, DeploymentId, LifecycleOwnerKind, RootKind, RootScope,
};
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, FixtureBuilder, RecordingSink};
use skill_studio_core::testing_shapes::with_plugin_cache_nesting;

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const TOTAL_SKILLS: usize = 378;
const RUNS: usize = 5;
const DOTAGENTS_SKILLS: usize = 16;
const SKILLS_SH_SKILLS: usize = 14;
const FORK_SKILLS: usize = 5;
const CURSOR_SKILLS: usize = 6;
/// Skills `with_plugin_cache_nesting` adds that the scan reports.
const PLUGIN_SKILLS: usize = 1;
const BASE_SKILLS: usize = TOTAL_SKILLS
    - DOTAGENTS_SKILLS
    - SKILLS_SH_SKILLS
    - FORK_SKILLS
    - CURSOR_SKILLS
    - PLUGIN_SKILLS;

fn skill_md(name: &str) -> String {
    let mut description = format!("Extra estate skill {name} helps with");
    while description.len() < 250 {
        description.push_str(" tidy review notes");
    }
    description.truncate(250);
    format!("---\nname: {name}\ndescription: {description}\n---\nBody text for {name}.\n")
}

fn with_universal_skill(b: FixtureBuilder, name: &str) -> FixtureBuilder {
    let dir = format!(".agents/skills/{name}");
    b.dir(&dir)
        .file(&format!("{dir}/SKILL.md"), skill_md(name).as_bytes())
}

/// The bench estate plus what it lacks: dotagents-owned, skills.sh-owned and
/// fork-owned shared skills, Cursor skills, and the plugin caches.
fn realistic_home(home: &Path) -> Vec<PathBuf> {
    let generated = estate(BASE_SKILLS, 1);
    let mut b = generated.builder;

    let mut agents_toml = String::new();
    let mut agents_lock = String::new();
    for i in 0..DOTAGENTS_SKILLS {
        let name = format!("dota-{i:02}-managed-skill");
        b = with_universal_skill(b, &name);
        writeln!(agents_toml, "[[skills]]\nname = \"{name}\"").unwrap();
        writeln!(agents_lock, "[skills.{name}]\nsource = \"owner/{name}\"").unwrap();
    }
    b = b
        .file(".agents/agents.toml", agents_toml.as_bytes())
        .file(".agents/agents.lock", agents_lock.as_bytes());

    let mut lock_skills = serde_json::Map::new();
    for i in 0..SKILLS_SH_SKILLS {
        let name = format!("ssh-{i:02}-store-skill");
        b = with_universal_skill(b, &name);
        lock_skills.insert(
            name,
            serde_json::json!({
                "source": "owner/repo",
                "sourceType": "github",
                "sourceUrl": "https://github.com/owner/repo",
                "skillFolderHash": "1111111111111111111111111111111111111111",
                "installedAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-02T00:00:00Z",
            }),
        );
    }
    let lock = serde_json::json!({ "version": 3, "skills": lock_skills });
    b = b.file(
        ".agents/.skill-lock.json",
        serde_json::to_vec(&lock).unwrap().as_slice(),
    );

    // Fork-owned skills are the removable ones that need no process spawner.
    let mut forks = serde_json::Map::new();
    for i in 0..FORK_SKILLS {
        let name = format!("fork-{i:02}-removable-skill");
        b = with_universal_skill(b, &name);
        forks.insert(name, serde_json::json!({}));
    }
    let registry = serde_json::json!({ "forks": forks });
    b = b.file(
        ".agents/skill-studio.json",
        serde_json::to_vec(&registry).unwrap().as_slice(),
    );

    for i in 0..CURSOR_SKILLS {
        let name = format!("cursor-{i:02}-own-skill");
        let dir = format!(".cursor/skills/{name}");
        b = b
            .dir(&dir)
            .file(&format!("{dir}/SKILL.md"), skill_md(&name).as_bytes());
    }
    b = with_plugin_cache_nesting(b);

    b.materialize(home).expect("materialize the timing home");
    generated.project_dirs
}

fn runtime_for(home: &Path, project_dirs: &[PathBuf]) -> Runtime {
    let mut scope = RuntimeScope::fixture(home);
    scope.read_timeout_ms = 10_000;
    scope.projects = ProjectSelection::Explicit {
        paths: project_dirs.iter().map(|d| home.join(d)).collect(),
    };
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history/events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&scope, ports).expect("runtime")
}

struct Row {
    job: &'static str,
    samples_ms: Vec<f64>,
}

fn time<T>(samples: &mut Vec<f64>, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    samples.push(start.elapsed().as_secs_f64() * 1000.0);
    out
}

fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[sorted.len() / 2]
}

fn max(samples: &[f64]) -> f64 {
    samples.iter().copied().fold(0.0, f64::max)
}

fn scan(rt: &Runtime) -> Inventory {
    ops::scan(rt, &ctx(), &ScanRequest::default()).unwrap()
}

/// A global, shared-folder skill nobody owns: the shape Park moves and
/// Unpark restores.
fn parkable_skill(inventory: &Inventory) -> (String, PathBuf) {
    inventory
        .skills
        .iter()
        .flat_map(|s| s.deployments.iter().map(move |d| (s, d)))
        .find(|(_, d)| {
            d.root.kind == RootKind::Universal
                && d.root.scope == RootScope::Global
                && d.backing == BackingRelationship::Canonical
                && d.owner_kind == LifecycleOwnerKind::Manual
        })
        .map(|(s, d)| (s.name.0.clone(), d.path.clone()))
        .expect("the estate has a parkable global shared skill")
}

fn deployment_id(inventory: &Inventory, name: &str, kind: &RootKind) -> DeploymentId {
    inventory
        .skills
        .iter()
        .find(|s| s.name.0 == name)
        .and_then(|s| s.deployments.iter().find(|d| d.root.kind == *kind))
        .unwrap_or_else(|| panic!("{name} has no {kind:?} deployment"))
        .id
        .clone()
}

/// `timing_on_a_378_skill_home_prints_each_job_duration`: the scan finds all
/// 378 skills, Park takes the skill out of its folder and Unpark brings it
/// back, and each job's duration is printed. A failed assertion means a job
/// returned the wrong result on a realistic home, not that it ran slowly.
#[test]
#[ignore = "timing run: cargo test -p skill-studio-core --all-features --test timing -- --ignored --nocapture"]
fn timing_on_a_378_skill_home_prints_each_job_duration() {
    let dir = unique_temp_dir("timing-378");
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.canonicalize().unwrap();
    let project_dirs = realistic_home(&home);

    let mut rows: Vec<Row> = Vec::new();

    let mut cold = Vec::new();
    let rt = runtime_for(&home, &project_dirs);
    let inventory = time(&mut cold, || scan(&rt));
    assert_eq!(
        inventory.skills.len(),
        TOTAL_SKILLS,
        "the scan must find every skill the home holds"
    );
    rows.push(Row {
        job: "scan, first run on a new runtime",
        samples_ms: cold,
    });

    let mut warm = Vec::new();
    for _ in 0..RUNS {
        let inventory = time(&mut warm, || scan(&rt));
        assert_eq!(inventory.skills.len(), TOTAL_SKILLS);
    }
    rows.push(Row {
        job: "scan, repeat on one runtime",
        samples_ms: warm,
    });

    let (name, universal_path) = parkable_skill(&inventory);
    let mut park_ms = Vec::new();
    let mut unpark_ms = Vec::new();
    for _ in 0..RUNS {
        let live = scan(&rt);
        let id = deployment_id(&live, &name, &RootKind::Universal);
        time(&mut park_ms, || {
            ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).unwrap()
        });
        assert!(
            std::fs::symlink_metadata(&universal_path).is_err(),
            "park must take {name} out of the shared folder"
        );

        let parked = scan(&rt);
        let parked_id = deployment_id(&parked, &name, &RootKind::Parked);
        time(&mut unpark_ms, || {
            ops::unpark(
                &rt,
                &ctx(),
                &UnparkRequest {
                    deployment_id: parked_id,
                },
            )
            .unwrap()
        });
        assert!(
            universal_path.join("SKILL.md").exists(),
            "unpark must bring {name} back to the shared folder"
        );
    }
    rows.push(Row {
        job: "park one shared skill",
        samples_ms: park_ms,
    });
    rows.push(Row {
        job: "unpark it",
        samples_ms: unpark_ms,
    });

    let mut remove_ms = Vec::new();
    for i in 0..FORK_SKILLS {
        let fork = format!("fork-{i:02}-removable-skill");
        let live = scan(&rt);
        let id = deployment_id(&live, &fork, &RootKind::Universal);
        time(&mut remove_ms, || {
            ops::remove(&rt, &ctx(), &RemoveRequest { deployment_id: id }).unwrap()
        });
        assert!(
            !home.join(".agents/skills").join(&fork).exists(),
            "remove must delete {fork} from the shared folder"
        );
    }
    rows.push(Row {
        job: "remove one fork-owned shared skill",
        samples_ms: remove_ms,
    });

    println!();
    println!(
        "{:<40} {:>10} {:>10} {:>7}",
        "job", "median ms", "max ms", "skills"
    );
    for row in &rows {
        println!(
            "{:<40} {:>10.1} {:>10.1} {:>7}",
            row.job,
            median(&row.samples_ms),
            max(&row.samples_ms),
            TOTAL_SKILLS
        );
    }
    println!();
    println!("runs per job: {RUNS} (first-run scan: 1)");
    println!("skipped: snapshot build - lives in the desktop crate, not reachable from core");
    println!("skipped: park for every agent on a dotagents skill - runs `dotagents remove` through the process spawner");
    println!(
        "skipped: remove of a manual skill - core refuses it; a fork-owned skill is timed instead"
    );
    println!("skipped: install, update, uninstall - run npx or the network");

    std::fs::remove_dir_all(&dir).ok();
}
