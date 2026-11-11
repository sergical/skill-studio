//! Release-only detection samples over a disposable home and fixture binaries.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]
use skill_studio_core::{
    dto::HarnessesRequest,
    harness::HarnessCatalog,
    identity::CorrelationId,
    ops,
    ports::{OpContext, Runtime},
    RuntimeScope,
};
use skill_studio_host::{default_ports_with_history, PathToolLookup, RealProcessSpawner};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

fn sample(home: &Path, bin: &Path, cache: Option<&Path>) -> serde_json::Value {
    let command_output = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let load = command_output("/usr/sbin/sysctl", &["-n", "vm.loadavg"]);
    let cargo = command_output("/usr/bin/pgrep", &["-x", "cargo"])
        .lines()
        .count();
    let rustc = command_output("/usr/bin/pgrep", &["-x", "rustc"])
        .lines()
        .count();
    let started = Instant::now();
    let scope = RuntimeScope::fixture(home);
    let catalog = Arc::new(HarnessCatalog::builtin());
    let mut ports = default_ports_with_history(
        home.join("leases"),
        catalog,
        scope.history_root.join("events.sqlite3"),
    );
    ports.tools = Some(Arc::new(PathToolLookup::with_search_dirs(vec![
        bin.to_path_buf()
    ])));
    let spawner = RealProcessSpawner::new();
    let spawner = if let Some(cache) = cache {
        spawner.with_probe_cache(cache.to_path_buf())
    } else {
        spawner
    };
    ports.spawner = Some(Arc::new(spawner));
    let rt = Runtime::new(&scope, ports).unwrap();
    let report = ops::harnesses(
        &rt,
        &OpContext::uncancellable(CorrelationId("performance".into())),
        &HarnessesRequest {},
    )
    .unwrap();
    assert!(
        report
            .harnesses
            .iter()
            .all(|row| row.version.value.is_some()),
        "{report:?}"
    );
    drop(rt);
    serde_json::json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,"load_average":load,"cargo_processes":cargo,"rustc_processes":rustc})
}

#[allow(clippy::assertions_on_constants)]
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("1.2.3");
        return;
    }
    assert!(!cfg!(debug_assertions), "use a release build");
    let root = PathBuf::from(std::env::args().nth(1).expect("fixture output directory"));
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("home");
    let bin = root.join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    let exe = std::env::current_exe().unwrap();
    for name in ["claude", "codex", "opencode", "pi", "cursor-agent", "grok"] {
        let target = bin.join(name);
        if !target.exists() {
            std::os::unix::fs::symlink(&exe, target).unwrap();
        }
    }
    let cache = root.join("probe-cache.json");
    let cache_enabled = std::env::args().nth(2).as_deref() != Some("uncached");
    let mut miss = Vec::new();
    let mut hit = Vec::new();
    for _ in 0..10 {
        let _ = std::fs::remove_file(&cache);
        miss.push(sample(
            &home,
            &bin,
            cache_enabled.then_some(cache.as_path()),
        ));
    }
    for _ in 0..10 {
        hit.push(sample(
            &home,
            &bin,
            cache_enabled.then_some(cache.as_path()),
        ));
    }
    println!(
        "{}",
        serde_json::json!({"cache_enabled":cache_enabled,"cache_miss_ms":miss,"cache_hit_ms":hit,"fixture":"six native version executables, empty agent config/session roots; new runtime and spawner each sample","n":10})
    );
}
