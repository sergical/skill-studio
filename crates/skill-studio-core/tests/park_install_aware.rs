// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

//! Real-disk tests for install-aware Park (#389): a parked skill must stay
//! parked when its installer runs again. The dotagents stub below stands in
//! for the real `dotagents`: `remove` edits `agents.toml` and `agents.lock`
//! the way the real command does, and `install` recreates every listed,
//! non-excluded skill. Each test names the flow, what must hold, and what a
//! failure means.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use skill_studio_core::dto::{
    DeploymentDto, InstallMethod, ListEventsRequest, ParkRequest, RestoreCapability,
    RestoreRequest, ScanRequest, UnparkRequest, UpdateRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{
    CancelToken, NeverCancel, Ports, ProcessOutput, ProcessSpawner, ProcessSpec, Runtime,
};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, FakeToolLookup, RecordingSink};
use skill_studio_core::CoreError;

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const DOTAGENTS: &str = "/fake/bin/dotagents";

/// One `dotagents` run the stub saw.
#[derive(Clone)]
struct Call {
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: Vec<(String, String)>,
}

/// A fake `dotagents` over the temp home. `wildcard_pool` is what a
/// `name = "*"` entry's source holds. Like the real command, `remove` asks
/// "Add to exclude list?" unless `-y` is given, and with no terminal it exits
/// 0 and changes nothing.
struct FakeDotagents {
    home: PathBuf,
    wildcard_pool: Vec<String>,
    calls: Mutex<Vec<Call>>,
    /// Exit code `remove` reports; non-zero also changes nothing.
    remove_exit: AtomicI32,
    /// Drops `-y` before `remove` looks at its arguments.
    ignore_yes: AtomicBool,
    /// Panics inside `remove`: the process dies after the copy moved.
    crash_in_remove: AtomicBool,
}

impl FakeDotagents {
    fn new(home: &Path, wildcard_pool: &[&str]) -> Arc<Self> {
        Arc::new(FakeDotagents {
            home: home.to_path_buf(),
            wildcard_pool: wildcard_pool.iter().map(|s| (*s).to_string()).collect(),
            calls: Mutex::new(Vec::new()),
            remove_exit: AtomicI32::new(0),
            ignore_yes: AtomicBool::new(false),
            crash_in_remove: AtomicBool::new(false),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.args.clone())
            .collect()
    }

    fn removes(&self) -> usize {
        self.calls()
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "remove"))
            .count()
    }

    fn last_call(&self) -> Call {
        self.calls.lock().unwrap().last().unwrap().clone()
    }

    fn toml_path(&self) -> PathBuf {
        self.home.join(".agents/agents.toml")
    }

    fn lock_path(&self) -> PathBuf {
        self.home.join(".agents/agents.lock")
    }

    fn skill_dir(&self, name: &str) -> PathBuf {
        self.home.join(".agents/skills").join(name)
    }

    fn write_lock_row(&self, name: &str) {
        let lock = std::fs::read_to_string(self.lock_path()).unwrap_or_default();
        if !lock.contains(&format!("[skills.{name}]")) {
            let lock = format!("{lock}[skills.{name}]\nsource = \"owner/{name}\"\n");
            std::fs::write(self.lock_path(), lock).unwrap();
        }
    }

    /// `remove` for the config in `dir`, whose skills live in `skills_dir`.
    /// A `name = "*"` entry that already excludes the skill is not the one
    /// that supplied it, so the exclude goes to the next one.
    fn remove(dir: &Path, skills_dir: &Path, name: &str) {
        let toml_path = dir.join("agents.toml");
        let lock_path = dir.join("agents.lock");
        let text = std::fs::read_to_string(&toml_path).unwrap();
        let mut doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let rows = doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
            .unwrap();
        let explicit = rows
            .iter()
            .position(|row| row.get("name").and_then(toml_edit::Item::as_str) == Some(name));
        if let Some(index) = explicit {
            rows.remove(index);
        } else {
            let wildcard = rows
                .iter_mut()
                .find(|row| {
                    row.get("name").and_then(toml_edit::Item::as_str) == Some("*")
                        && !row
                            .get("exclude")
                            .and_then(toml_edit::Item::as_array)
                            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(name)))
                })
                .unwrap();
            if wildcard.get("exclude").is_none() {
                wildcard["exclude"] = toml_edit::value(toml_edit::Array::new());
            }
            wildcard["exclude"]
                .as_array_mut()
                .unwrap()
                .push(name.to_string());
        }
        std::fs::write(&toml_path, doc.to_string()).unwrap();
        std::fs::remove_dir_all(skills_dir.join(name)).ok();
        let lock = std::fs::read_to_string(&lock_path).unwrap_or_default();
        let mut lock_doc = lock.parse::<toml_edit::DocumentMut>().unwrap();
        if let Some(skills) = lock_doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_table_mut)
        {
            skills.remove(name);
        }
        std::fs::write(&lock_path, lock_doc.to_string()).unwrap();
    }

    fn install(&self) {
        let text = std::fs::read_to_string(self.toml_path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let mut wanted: Vec<String> = Vec::new();
        for row in doc
            .get("skills")
            .and_then(toml_edit::Item::as_array_of_tables)
            .unwrap()
        {
            match row.get("name").and_then(toml_edit::Item::as_str).unwrap() {
                "*" => {
                    let excluded: Vec<&str> = row
                        .get("exclude")
                        .and_then(toml_edit::Item::as_array)
                        .map(|list| list.iter().filter_map(|item| item.as_str()).collect())
                        .unwrap_or_default();
                    wanted.extend(
                        self.wildcard_pool
                            .iter()
                            .filter(|name| !excluded.contains(&name.as_str()))
                            .cloned(),
                    );
                }
                name => wanted.push(name.to_string()),
            }
        }
        for name in wanted {
            let dir = self.skill_dir(&name);
            if !dir.exists() {
                write_skill(&dir, &name);
            }
            self.write_lock_row(&name);
        }
    }
}

fn env_value<'a>(spec: &'a ProcessSpec, key: &str) -> Option<&'a str> {
    spec.env
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

impl ProcessSpawner for FakeDotagents {
    fn run(
        &self,
        spec: &ProcessSpec,
        _cancel: &dyn CancelToken,
    ) -> Result<ProcessOutput, CoreError> {
        assert_eq!(spec.program, DOTAGENTS);
        self.calls.lock().unwrap().push(Call {
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            env: spec.env.clone(),
        });
        let mut status = 0;
        match spec.args.iter().position(|arg| arg == "remove") {
            Some(index) => {
                assert!(
                    !self.crash_in_remove.load(Ordering::SeqCst),
                    "the process died inside dotagents remove"
                );
                status = self.remove_exit.load(Ordering::SeqCst);
                let confirmed = spec.args.iter().any(|arg| arg == "-y")
                    && !self.ignore_yes.load(Ordering::SeqCst);
                if status == 0 && confirmed {
                    let name = &spec.args[index + 1];
                    if spec.args.iter().any(|arg| arg == "--project") {
                        let project = spec.cwd.clone().unwrap();
                        assert_eq!(env_value(spec, "DOTAGENTS_HOME"), Some(""));
                        Self::remove(&project, &project.join(".agents/skills"), name);
                    } else {
                        let dir = PathBuf::from(env_value(spec, "DOTAGENTS_HOME").unwrap());
                        Self::remove(&dir, &dir.join("skills"), name);
                    }
                }
            }
            None => self.install(),
        }
        Ok(ProcessOutput {
            status: Some(status),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

/// Runs `dotagents install` through the stub, as a person would in a terminal.
fn dotagents_install(stub: &FakeDotagents) {
    let spec = ProcessSpec {
        program: DOTAGENTS.to_string(),
        args: vec!["install".to_string()],
        cwd: None,
        env: Vec::new(),
        timeout_ms: 1_000,
    };
    stub.run(&spec, &NeverCancel).unwrap();
}

fn write_skill(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: a parkable skill\n---\nBody.\n"),
    )
    .unwrap();
}

fn runtime(home: &Path, spawner: Arc<dyn ProcessSpawner>, dotagents_on_path: bool) -> Runtime {
    runtime_with_projects(home, spawner, dotagents_on_path, Vec::new())
}

fn runtime_with_projects(
    home: &Path,
    spawner: Arc<dyn ProcessSpawner>,
    dotagents_on_path: bool,
    projects: Vec<PathBuf>,
) -> Runtime {
    let mut tools = FakeToolLookup::default();
    if dotagents_on_path {
        tools
            .binaries
            .insert("dotagents".to_string(), PathBuf::from(DOTAGENTS));
    }
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: Some(spawner),
        discovery: None,
        tools: Some(Arc::new(tools)),
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    let mut scope = RuntimeScope::fixture(home);
    scope.projects = ProjectSelection::Explicit { paths: projects };
    Runtime::new(&scope, ports).unwrap()
}

fn copies(rt: &Runtime, name: &str) -> Vec<DeploymentDto> {
    ops::scan(rt, &ctx(), &ScanRequest::default())
        .unwrap()
        .skills
        .into_iter()
        .filter(|skill| skill.name.0 == name)
        .flat_map(|skill| skill.deployments)
        .collect()
}

fn live_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    copies(rt, name)
        .into_iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .unwrap_or_else(|| panic!("no live Universal copy of {name}"))
}

fn live_project_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    copies(rt, name)
        .into_iter()
        .find(|d| {
            matches!(d.root.scope, RootScope::Project(_)) && d.root.kind == RootKind::Universal
        })
        .unwrap_or_else(|| panic!("no live project copy of {name}"))
}

fn parked_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    copies(rt, name)
        .into_iter()
        .find(|d| d.root.kind == RootKind::Parked)
        .unwrap_or_else(|| panic!("no parked copy of {name}"))
}

const WILDCARD_TOML: &str =
    "# my dotagents setup\nversion = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n";
const EXPLICIT_TOML: &str = "# my dotagents setup\nversion = 1\n\n[[skills]]\nname = \"foo\"\nsource = \"owner/foo\"\n\n[[skills]]\nname = \"bar\"\nsource = \"owner/bar\"\n";
/// Two wildcard entries; the first one excludes `foo` on purpose.
const TWO_WILDCARDS_TOML: &str = "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/mine\"\nexclude = [\"foo\"]\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n";
const FOO_LOCK: &str = "[skills.foo]\nsource = \"owner/foo\"\n";

/// A temp home where dotagents has installed `foo` and `bar` from `toml`.
fn dotagents_home(label: &str, toml: &str) -> (PathBuf, Arc<FakeDotagents>) {
    let home = unique_temp_dir(label);
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(home.join(".agents/agents.toml"), toml).unwrap();
    let stub = FakeDotagents::new(&home, &["foo", "bar"]);
    dotagents_install(&stub);
    (home, stub)
}

/// A project at `project` whose `agents.toml` lists `foo`, with a live copy.
fn project_with_foo(project: &Path) {
    std::fs::create_dir_all(project).unwrap();
    std::fs::write(project.join("agents.toml"), EXPLICIT_TOML).unwrap();
    std::fs::write(project.join("agents.lock"), FOO_LOCK).unwrap();
    write_skill(&project.join(".agents/skills/foo"), "foo");
}

fn park_foo(rt: &Runtime) -> skill_studio_core::dto::ParkOutcome {
    ops::park(
        rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(rt, "foo").id,
        },
    )
    .unwrap()
}

fn unpark_foo(rt: &Runtime) {
    ops::unpark(
        rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(rt, "foo").id,
        },
    )
    .unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn statuses(rt: &Runtime) -> Vec<String> {
    ops::list_events(rt, &ctx(), &ListEventsRequest::default())
        .unwrap()
        .into_iter()
        .map(|event| event.status)
        .collect()
}

/// Every path under `dir`, so a test can see a marker or folder left behind.
fn paths_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        found.extend(paths_under(&path));
        found.push(path);
    }
    found
}

/// Flow: park a skill a `name = "*"` entry supplies, run `dotagents install`,
/// then turn the skill on and run it again. Expectation: park runs
/// `remove foo -y` with `DOTAGENTS_HOME` set, install leaves the parked skill
/// parked, turn-on takes it out of `exclude` and puts its lock row back, and
/// the next install keeps it. Failure: dotagents brings a parked skill back,
/// or turn-on leaves the skill excluded for good.
#[test]
fn wildcard_skill_stays_parked_through_dotagents_install_and_returns_on_turn_on() {
    let (home, stub) = dotagents_home("park_dotagents_wildcard", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let live = home.join(".agents/skills/foo");

    park_foo(&rt);
    assert_eq!(stub.calls().last().unwrap(), &["remove", "foo", "-y"]);
    let call = stub.last_call();
    assert_eq!(call.cwd.as_deref(), Some(home.as_path()));
    assert!(call.env.contains(&(
        "DOTAGENTS_HOME".to_string(),
        home.join(".agents").display().to_string()
    )));
    assert!(read(&stub.toml_path()).contains("exclude"));
    dotagents_install(&stub);
    assert!(
        !live.exists(),
        "dotagents install brought the parked skill back"
    );
    assert!(home.join(".agents/skills/bar").exists());
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());

    unpark_foo(&rt);
    assert_eq!(
        read(&stub.toml_path()),
        WILDCARD_TOML,
        "turn-on must restore agents.toml as it was"
    );
    assert!(read(&stub.lock_path()).contains("[skills.foo]"));
    std::fs::remove_dir_all(&live).unwrap();
    dotagents_install(&stub);
    assert!(
        live.join("SKILL.md").exists(),
        "after turn-on dotagents install must manage the skill again"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: same as the wildcard test, for a skill `agents.toml` lists by name.
/// Expectation: park drops the entry and keeps the rest of the file (comment
/// and the `bar` entry); turn-on lists `foo` again. Failure: install revives
/// the skill, a neighbouring entry or comment is lost, or the entry does not
/// come back.
#[test]
fn explicit_skill_stays_parked_through_dotagents_install_and_returns_on_turn_on() {
    let (home, stub) = dotagents_home("park_dotagents_explicit", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let live = home.join(".agents/skills/foo");

    park_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(!toml.contains("name = \"foo\""));
    assert!(toml.contains("# my dotagents setup") && toml.contains("name = \"bar\""));
    dotagents_install(&stub);
    assert!(
        !live.exists(),
        "dotagents install brought the parked skill back"
    );

    unpark_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(toml.contains("name = \"foo\"") && toml.contains("source = \"owner/foo\""));
    assert!(toml.contains("# my dotagents setup") && toml.contains("name = \"bar\""));
    std::fs::remove_dir_all(&live).unwrap();
    dotagents_install(&stub);
    assert!(
        live.join("SKILL.md").exists(),
        "after turn-on dotagents install must manage the skill again"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill, turn it on, park it again, with no install between.
/// Expectation: turn-on writes the skill's `agents.lock` row back, so the
/// second park still sees a dotagents skill and runs `remove` again. Failure:
/// the lock row stays deleted, the second park skips `remove`, and
/// `dotagents install` brings the skill back.
#[test]
fn park_turn_on_park_again_runs_the_remove_each_time() {
    for toml in [WILDCARD_TOML, EXPLICIT_TOML] {
        let (home, stub) = dotagents_home("park_dotagents_twice", toml);
        let rt = runtime(&home, stub.clone(), true);

        park_foo(&rt);
        unpark_foo(&rt);
        assert!(read(&stub.lock_path()).contains("[skills.foo]"));
        park_foo(&rt);

        assert_eq!(
            stub.removes(),
            2,
            "the second park skipped dotagents remove"
        );
        dotagents_install(&stub);
        assert!(
            !home.join(".agents/skills/foo").exists(),
            "dotagents install brought the twice-parked skill back"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: two `name = "*"` entries, the first of which excludes `foo` because
/// the person wrote it. Park `foo`, then turn it on. Expectation: dotagents
/// excludes `foo` in the second entry, turn-on lifts it from that entry only,
/// and the file ends as it began. Failure: turn-on clears the person's own
/// exclude and `foo` is installed from a source they excluded.
#[test]
fn turn_on_lifts_only_the_exclude_that_park_added() {
    let (home, stub) = dotagents_home("park_dotagents_two_wildcards", TWO_WILDCARDS_TOML);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let parked = read(&stub.toml_path());
    assert_eq!(parked.matches("exclude").count(), 2, "{parked}");

    unpark_foo(&rt);

    assert_eq!(read(&stub.toml_path()), TWO_WILDCARDS_TOML);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a dotagents skill, then ask for its journal row to be undone.
/// Expectation: the park and the turn-on rows report no inverse and undo is
/// refused; turn-on is the way back. Failure: an undo of the park rewrites
/// `agents.toml` behind the app's back, or a row claims it can be restored.
#[test]
fn a_dotagents_park_row_has_no_inverse() {
    let (home, stub) = dotagents_home("park_dotagents_no_inverse", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);

    let parked = park_foo(&rt);
    let parked_toml = read(&stub.toml_path());
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert_eq!(events[0].restore, RestoreCapability::NoInverse);

    let result = ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: parked.event_id,
            force: false,
        },
    );

    assert!(result.is_err());
    assert_eq!(read(&stub.toml_path()), parked_toml);
    unpark_foo(&rt);
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert_eq!(events[0].restore, RestoreCapability::NoInverse);
    std::fs::remove_dir_all(&home).ok();
}

/// Parks `foo` after `setup` made the stub misbehave, and checks the park
/// rolled back: the copy is at its place, both dotagents files are
/// byte-identical, the row is failed, and no folder or marker stays parked.
fn assert_failed_park_rolls_back(label: &str, setup: impl Fn(&FakeDotagents), expected: &str) {
    for toml in [WILDCARD_TOML, EXPLICIT_TOML] {
        let (home, stub) = dotagents_home(label, toml);
        let rt = runtime(&home, stub.clone(), true);
        let (toml_before, lock_before) = (read(&stub.toml_path()), read(&stub.lock_path()));
        setup(&stub);

        let err = ops::park(
            &rt,
            &ctx(),
            &ParkRequest {
                deployment_id: live_copy(&rt, "foo").id,
            },
        )
        .unwrap_err();

        assert!(err.message.contains(expected), "{}", err.message);
        assert!(home.join(".agents/skills/foo/SKILL.md").exists());
        assert_eq!(read(&stub.toml_path()), toml_before);
        assert_eq!(read(&stub.lock_path()), lock_before);
        assert_eq!(statuses(&rt), ["failed"]);
        let left: Vec<PathBuf> = paths_under(&home.join(".agents/skills-parked"))
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| name == "foo"))
            .collect();
        assert!(left.is_empty(), "park left {left:?} behind");
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: `dotagents remove` exits non-zero. Expectation: the copy is moved
/// back, `agents.toml` and `agents.lock` are byte-identical to before, the row
/// is failed, and no marker is left. Failure: the skill ends parked with the
/// file edits half done, or a stale marker misleads a later turn-on.
#[test]
fn a_failed_dotagents_remove_puts_the_copy_and_both_files_back() {
    assert_failed_park_rolls_back(
        "park_dotagents_remove_fails",
        |stub| stub.remove_exit.store(3, Ordering::SeqCst),
        "dotagents remove",
    );
}

/// Flow: `dotagents remove` exits 0 but changes nothing, as it does when it
/// must ask a question and has no terminal. Expectation: park notices that
/// `agents.toml` still lists the skill, fails, and rolls back. Failure: park
/// reports success and the next `dotagents install` brings the copy back.
#[test]
fn a_dotagents_remove_that_changes_nothing_fails_the_park() {
    assert_failed_park_rolls_back(
        "park_dotagents_no_change",
        |stub| stub.ignore_yes.store(true, Ordering::SeqCst),
        "still lists foo",
    );
}

/// Flow: park a project skill dotagents manages. Expectation: the call is
/// `--project remove foo -y` in the project folder with no `DOTAGENTS_HOME`,
/// the project's `agents.toml` changes, and the global one does not. Failure:
/// the remove edits the global config, or runs outside the project.
#[test]
fn project_park_runs_in_the_project_and_leaves_the_global_config_alone() {
    let home = unique_temp_dir("park_dotagents_project");
    let project = home.join("proj");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    project_with_foo(&project);
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(home.join(".agents/agents.toml"), WILDCARD_TOML).unwrap();
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime_with_projects(&home, stub.clone(), true, vec![project.clone()]);
    let live = live_project_copy(&rt, "foo");

    ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live.id,
        },
    )
    .unwrap();

    assert_eq!(
        stub.calls().last().unwrap(),
        &["--project", "remove", "foo", "-y"]
    );
    let call = stub.last_call();
    assert_eq!(call.cwd.as_deref(), Some(project.as_path()));
    assert!(call
        .env
        .contains(&("DOTAGENTS_HOME".to_string(), String::new())));
    assert!(!read(&project.join("agents.toml")).contains("name = \"foo\""));
    assert!(!read(&project.join("agents.lock")).contains("[skills.foo]"));
    assert_eq!(read(&home.join(".agents/agents.toml")), WILDCARD_TOML);
    assert!(!project.join(".agents/skills/foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill from a project folder that sits inside a bigger git
/// repository. Expectation: park is refused with a reason that names the git
/// root, and nothing runs. Failure: dotagents edits the repository's
/// `agents.toml` while the app thinks it changed the project's.
#[test]
fn project_park_inside_a_larger_repository_is_refused() {
    let home = unique_temp_dir("park_dotagents_nested");
    let repo = home.join("repo");
    let project = repo.join("packages/app");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    project_with_foo(&project);
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime_with_projects(&home, stub.clone(), true, vec![project.clone()]);
    let live = live_project_copy(&rt, "foo");

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("git root"), "{}", err.message);
    assert!(err.message.contains(&repo.display().to_string()));
    assert!(stub.calls().is_empty());
    assert!(project.join(".agents/skills/foo/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn-on edits `agents.toml` and `agents.lock`, then the folder cannot
/// move back (its target folder is read-only). Expectation: the error says so,
/// both files are written back to their parked state, the copy stays parked,
/// the row is failed, and a retry after the fix succeeds. Failure: the files
/// list the skill while the copy is parked, and the retry cannot run.
#[cfg(unix)]
#[test]
fn a_turn_on_whose_move_fails_writes_both_files_back_and_can_be_retried() {
    use std::os::unix::fs::PermissionsExt;
    let (home, stub) = dotagents_home("park_dotagents_turn_on_fails", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    let (toml_parked, lock_parked) = (read(&stub.toml_path()), read(&stub.lock_path()));
    let skills = home.join(".agents/skills");
    std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo").id,
        },
    );
    std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(result.is_err());
    assert_eq!(read(&stub.toml_path()), toml_parked);
    assert_eq!(read(&stub.lock_path()), lock_parked);
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    assert_eq!(statuses(&rt)[0], "failed");
    unpark_foo(&rt);
    assert!(read(&stub.toml_path()).contains("name = \"foo\""));
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the process dies after the copy moved and before `dotagents remove`
/// ran, so the park row stays pending. The next operation marks it
/// interrupted; then the person turns the skill on. Expectation: the copy
/// comes back and `agents.toml` and `agents.lock` do not change (the skill was
/// never removed from them). Failure: turn-on needs the finished park, or it
/// rewrites files the park never touched.
#[test]
fn a_park_that_crashed_after_the_move_can_still_be_turned_on() {
    let (home, stub) = dotagents_home("park_dotagents_crash", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let (toml_before, lock_before) = (read(&stub.toml_path()), read(&stub.lock_path()));
    stub.crash_in_remove.store(true, Ordering::SeqCst);
    let deployment_id = live_copy(&rt, "foo").id;

    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ops::park(&rt, &ctx(), &ParkRequest { deployment_id })
    }));
    assert!(crashed.is_err());
    assert!(!home.join(".agents/skills/foo").exists());
    stub.crash_in_remove.store(false, Ordering::SeqCst);

    unpark_foo(&rt);

    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert_eq!(read(&stub.toml_path()), toml_before);
    assert_eq!(read(&stub.lock_path()), lock_before);
    assert_eq!(statuses(&rt), ["done", "interrupted"]);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a skill was parked before this build ran `dotagents remove` (the
/// copy sits in `skills-parked` but `agents.toml` still lists it), then the app
/// runs a dotagents Update for another skill. Expectation: Update is refused
/// with the exact plain reason and `dotagents install` never runs. Failure:
/// install copies the parked skill back.
#[test]
fn dotagents_update_is_refused_while_a_parked_skill_is_still_listed() {
    let (home, stub) = dotagents_home("park_dotagents_old_park", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);
    std::fs::create_dir_all(home.join(".agents/skills-parked/universal")).unwrap();
    std::fs::rename(
        home.join(".agents/skills/foo"),
        home.join(".agents/skills-parked/universal/foo"),
    )
    .unwrap();
    let calls_before = stub.calls().len();

    let err = ops::update(
        &rt,
        &ctx(),
        &UpdateRequest {
            skill: SkillName("bar".to_string()),
            method: InstallMethod::Dotagents,
            scope: RootScope::Global,
            files: Vec::new(),
            source: None,
            ref_pin: None,
        },
    )
    .unwrap_err();

    assert_eq!(
        err.message,
        "foo is parked but still listed in agents.toml, so dotagents install would bring it back. Turn it on or remove it from agents.toml first."
    );
    assert_eq!(stub.calls().len(), calls_before);
    assert!(!home.join(".agents/skills/foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a dotagents skill on a machine with no `dotagents` on `PATH`.
/// Expectation: park is refused with a reason that names dotagents, the copy
/// stays where it is, `agents.toml` is untouched, and nothing ran. Failure:
/// the copy is parked anyway and the next `dotagents install` brings it back.
#[test]
fn park_of_a_dotagents_skill_is_refused_with_a_plain_reason_when_dotagents_is_missing() {
    let (home, stub) = dotagents_home("park_dotagents_missing", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), false);
    let calls_before = stub.calls().len();

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    )
    .unwrap_err();

    assert!(
        err.message.contains("but is not installed here"),
        "{}",
        err.message
    );
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert_eq!(read(&stub.toml_path()), WILDCARD_TOML);
    assert_eq!(stub.calls().len(), calls_before);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill the skills CLI installed, then ask the app to update
/// it. Expectation: the update is refused with a plain reason, the CLI never
/// runs, the folder is not recreated, and the parked copy and
/// `.skill-lock.json` stay as they were. Failure: Update re-fetches a parked
/// skill and undoes the park.
#[test]
fn app_update_skips_a_parked_skills_cli_skill() {
    let home = unique_temp_dir("park_skills_cli_update");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let lock = "{\"version\":3,\"skills\":{\"foo\":{\"source\":\"owner/foo\",\"sourceType\":\"github\",\"skillFolderHash\":\"abc\"}}}";
    std::fs::write(home.join(".agents/.skill-lock.json"), lock).unwrap();
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let err = ops::update(
        &rt,
        &ctx(),
        &UpdateRequest {
            skill: SkillName("foo".to_string()),
            method: InstallMethod::SkillsSh,
            scope: RootScope::Global,
            files: Vec::new(),
            source: None,
            ref_pin: None,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("is parked"), "{}", err.message);
    assert!(stub.calls().is_empty(), "the CLI ran for a parked skill");
    assert!(!live.exists());
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    assert_eq!(read(&home.join(".agents/.skill-lock.json")), lock);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a copy placed by hand. Expectation: no installer is called.
/// Failure: park shells out for a skill no installer manages.
#[test]
fn park_of_a_hand_placed_skill_runs_no_installer() {
    let home = unique_temp_dir("park_by_hand");
    write_skill(&home.join(".agents/skills/foo"), "foo");
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);

    assert!(stub.calls().is_empty());
    std::fs::remove_dir_all(&home).ok();
}
