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
const NPX: &str = "/fake/bin/npx";

/// One `dotagents` run the stub saw.
#[derive(Clone)]
struct Call {
    program: String,
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
    /// `remove` leaves the skill's `agents.lock` row in place.
    keep_lock_row: AtomicBool,
    /// `remove` never writes `agents.toml`, as real dotagents does with a
    /// multi-line `exclude` or CRLF line endings.
    leave_toml: AtomicBool,
    /// A failed `remove` leaves a folder at the skill's own path, so the
    /// rollback cannot move the parked copy back.
    block_move_back: AtomicBool,
    /// `remove` also writes an unrelated skill into `agents.toml`, as another
    /// tool editing the file at the same time would.
    add_unrelated_entry: AtomicBool,
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
            keep_lock_row: AtomicBool::new(false),
            leave_toml: AtomicBool::new(false),
            block_move_back: AtomicBool::new(false),
            add_unrelated_entry: AtomicBool::new(false),
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

    fn write_lock_row(&self, name: &str, source: &str) {
        let lock = std::fs::read_to_string(self.lock_path()).unwrap_or_default();
        if !lock.contains(&format!("[skills.{name}]")) {
            let lock = format!("{lock}[skills.{name}]\nsource = \"{source}\"\n");
            std::fs::write(self.lock_path(), lock).unwrap();
        }
    }

    /// `remove` for the config in `dir`, whose skills live in `skills_dir`.
    /// Like the real command, a `name = "*"` entry supplies the skill only
    /// when its `source` equals the skill's `agents.lock` source and it does
    /// not exclude the name; the exclude goes to the first such entry, and
    /// is not written when that entry's list spans several lines.
    fn remove(dir: &Path, skills_dir: &Path, name: &str, keep_lock_row: bool, leave_toml: bool) {
        let toml_path = dir.join("agents.toml");
        let lock_path = dir.join("agents.lock");
        let text = std::fs::read_to_string(&toml_path).unwrap();
        let mut doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let lock = std::fs::read_to_string(&lock_path).unwrap_or_default();
        let mut lock_doc = lock.parse::<toml_edit::DocumentMut>().unwrap();
        let locked_source = lock_doc
            .get("skills")
            .and_then(toml_edit::Item::as_table)
            .and_then(|skills| skills.get(name))
            .and_then(|row| row.get("source"))
            .and_then(toml_edit::Item::as_str)
            .map(str::to_string);
        let rows = doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
            .unwrap();
        let explicit = rows
            .iter()
            .position(|row| row.get("name").and_then(toml_edit::Item::as_str) == Some(name));
        let mut write_toml = !leave_toml;
        if let Some(index) = explicit {
            rows.remove(index);
        } else {
            let wildcard = rows
                .iter_mut()
                .find(|row| {
                    row.get("name").and_then(toml_edit::Item::as_str) == Some("*")
                        && row.get("source").and_then(toml_edit::Item::as_str)
                            == locked_source.as_deref()
                        && !row
                            .get("exclude")
                            .and_then(toml_edit::Item::as_array)
                            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(name)))
                })
                .unwrap();
            if wildcard.get("exclude").is_none() {
                wildcard["exclude"] = toml_edit::value(toml_edit::Array::new());
            }
            let exclude = wildcard["exclude"].as_array_mut().unwrap();
            if exclude.to_string().contains('\n') {
                write_toml = false;
            } else {
                exclude.push(name.to_string());
            }
        }
        if write_toml {
            std::fs::write(&toml_path, doc.to_string()).unwrap();
        }
        std::fs::remove_dir_all(skills_dir.join(name)).ok();
        let gitignore = skills_dir.with_file_name(".gitignore");
        if let Ok(text) = std::fs::read_to_string(&gitignore) {
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| *line != format!("/skills/{name}"))
                .collect();
            std::fs::write(&gitignore, format!("{}\n", kept.join("\n"))).unwrap();
        }
        if !keep_lock_row {
            if let Some(skills) = lock_doc
                .get_mut("skills")
                .and_then(toml_edit::Item::as_table_mut)
            {
                skills.remove(name);
                // dotagents leaves the emptied table behind as a bare `[skills]`.
                skills.set_implicit(false);
            }
            std::fs::write(&lock_path, lock_doc.to_string()).unwrap();
        }
    }

    fn install(&self) {
        let text = std::fs::read_to_string(self.toml_path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let mut wanted: Vec<(String, String)> = Vec::new();
        for row in doc
            .get("skills")
            .and_then(toml_edit::Item::as_array_of_tables)
            .unwrap()
        {
            let source = row
                .get("source")
                .and_then(toml_edit::Item::as_str)
                .unwrap()
                .to_string();
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
                            .map(|name| (name.clone(), source.clone())),
                    );
                }
                name => wanted.push((name.to_string(), source)),
            }
        }
        for (name, source) in wanted {
            let dir = self.skill_dir(&name);
            if !dir.exists() {
                write_skill(&dir, &name);
            }
            self.write_lock_row(&name, &source);
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
        let mut spec = spec.clone();
        if spec.program == NPX {
            let wrapper: Vec<String> = spec.args.drain(..2).collect();
            assert_eq!(wrapper, ["-y", "@sentry/dotagents"]);
        } else {
            assert_eq!(spec.program, DOTAGENTS);
        }
        let spec = &spec;
        self.calls.lock().unwrap().push(Call {
            program: spec.program.clone(),
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
                        Self::remove(
                            &project,
                            &project.join(".agents/skills"),
                            name,
                            self.keep_lock_row.load(Ordering::SeqCst),
                            self.leave_toml.load(Ordering::SeqCst),
                        );
                    } else {
                        let dir = PathBuf::from(env_value(spec, "DOTAGENTS_HOME").unwrap());
                        Self::remove(
                            &dir,
                            &dir.join("skills"),
                            name,
                            self.keep_lock_row.load(Ordering::SeqCst),
                            self.leave_toml.load(Ordering::SeqCst),
                        );
                    }
                }
            }
            None => self.install(),
        }
        if status == 0 && self.add_unrelated_entry.load(Ordering::SeqCst) {
            let toml = std::fs::read_to_string(self.toml_path()).unwrap();
            let toml = toml + "\n[[skills]]\nname = \"extra\"\nsource = \"owner/extra\"\n";
            std::fs::write(self.toml_path(), toml).unwrap();
        }
        if status != 0 && self.block_move_back.load(Ordering::SeqCst) {
            write_skill(&self.skill_dir("foo"), "foo");
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
    let binaries: &[(&str, &str)] = if dotagents_on_path {
        &[("dotagents", DOTAGENTS)]
    } else {
        &[]
    };
    runtime_with_tools(home, spawner, binaries, projects)
}

fn runtime_with_tools(
    home: &Path,
    spawner: Arc<dyn ProcessSpawner>,
    binaries: &[(&str, &str)],
    projects: Vec<PathBuf>,
) -> Runtime {
    let mut tools = FakeToolLookup::default();
    for (name, path) in binaries {
        tools
            .binaries
            .insert((*name).to_string(), PathBuf::from(path));
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
const ONLY_FOO_TOML: &str = "version = 1\n\n[[skills]]\nname = \"foo\"\nsource = \"owner/foo\"\n";
const FOO_LOCK: &str = "[skills.foo]\nsource = \"owner/foo\"\n";

/// A temp home where dotagents has installed `foo` and `bar` from `toml`.
fn dotagents_home(label: &str, toml: &str) -> (PathBuf, Arc<FakeDotagents>) {
    dotagents_home_with_pool(label, toml, &["foo", "bar"])
}

/// Like [`dotagents_home`], with `pool` as what a `name = "*"` source holds.
fn dotagents_home_with_pool(
    label: &str,
    toml: &str,
    pool: &[&str],
) -> (PathBuf, Arc<FakeDotagents>) {
    let home = unique_temp_dir(label);
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(home.join(".agents/agents.toml"), toml).unwrap();
    let stub = FakeDotagents::new(&home, pool);
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
fn assert_failed_park_rolls_back(
    label: &str,
    tomls: &[&str],
    setup: impl Fn(&FakeDotagents),
    expected: &str,
) {
    for toml in tomls.iter().copied() {
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
        &[WILDCARD_TOML, EXPLICIT_TOML],
        |stub| stub.remove_exit.store(3, Ordering::SeqCst),
        "dotagents remove",
    );
}

/// Flow: `dotagents remove` exits 0 but changes nothing, as it does when it
/// must ask a question and has no terminal. Expectation: park notices that
/// `agents.toml` still lists the skill, fails, and rolls back. Failure: park
/// reports success and the next `dotagents install` brings the copy back.
/// A wildcard skill is different: park adds the exclude itself (see the
/// repair tests below), so only an explicit entry can fail this way.
#[test]
fn a_dotagents_remove_that_changes_nothing_fails_the_park() {
    assert_failed_park_rolls_back(
        "park_dotagents_no_change",
        &[EXPLICIT_TOML],
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

/// A `name = "*"` entry in front of the one that supplies `foo`, to be removed.
const ROWS_BEFORE_WILDCARD_TOML: &str = "version = 1\n\n[[skills]]\nname = \"bar\"\nsource = \"owner/bar\"\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n";
/// The `exclude` list of the only wildcard entry spans several lines.
const MULTILINE_EXCLUDE_TOML: &str = "# setup\nversion = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\nexclude = [\n  \"baz\",\n]\n";

/// Flow: no `dotagents` on `PATH`, but `npx` is. Expectation: park runs
/// `npx -y @sentry/dotagents remove foo -y` and the skill is parked. Failure:
/// park refuses a machine that can run dotagents through `npx`.
#[test]
fn park_falls_back_to_npx_when_dotagents_is_not_on_path() {
    let (home, stub) = dotagents_home("park_dotagents_npx", WILDCARD_TOML);
    let rt = runtime_with_tools(&home, stub.clone(), &[("npx", NPX)], Vec::new());

    park_foo(&rt);

    let call = stub.last_call();
    assert_eq!(call.program, NPX);
    assert_eq!(call.args, ["remove", "foo", "-y"]);
    assert!(read(&stub.toml_path()).contains("exclude"));
    assert!(!home.join(".agents/skills/foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a wildcard skill, then the person deletes the `[[skills]]` row
/// in front of the wildcard entry, then turn the skill on. Expectation: the
/// wildcard entry (found by its source) loses the exclude and the file ends
/// as the person left it. Failure: turn-on edits the row that now sits at the
/// old position, or leaves `foo` excluded.
#[test]
fn turn_on_finds_the_wildcard_entry_after_an_earlier_row_was_removed() {
    let (home, stub) =
        dotagents_home_with_pool("park_dotagents_moved", ROWS_BEFORE_WILDCARD_TOML, &["foo"]);
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    let parked = read(&stub.toml_path());
    let mut doc = parked.parse::<toml_edit::DocumentMut>().unwrap();
    doc["skills"].as_array_of_tables_mut().unwrap().remove(0);
    std::fs::write(stub.toml_path(), doc.to_string()).unwrap();

    unpark_foo(&rt);

    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("name = \"*\"") && !toml.contains("exclude"),
        "{toml}"
    );
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a wildcard skill, then the person deletes every entry that
/// could supply it, then turn the skill on. Expectation: turn-on fails before
/// it writes anything, the copy stays parked, and both files are byte for
/// byte as they were. Failure: the lock row is written back for a skill
/// dotagents will never install, or the copy moves out of the park.
#[test]
fn turn_on_fails_cleanly_when_no_entry_supplies_the_skill_any_more() {
    let (home, stub) =
        dotagents_home_with_pool("park_dotagents_gone", ROWS_BEFORE_WILDCARD_TOML, &["foo"]);
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    std::fs::write(stub.toml_path(), "version = 1\n").unwrap();
    let lock_parked = read(&stub.lock_path());

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo").id,
        },
    );

    let err = result.unwrap_err();
    assert!(
        err.message.contains("no longer has an entry"),
        "{}",
        err.message
    );
    assert_eq!(read(&stub.toml_path()), "version = 1\n");
    assert_eq!(read(&stub.lock_path()), lock_parked);
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    assert!(!home.join(".agents/skills/foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill whose wildcard entry has a multi-line `exclude`, where
/// real dotagents leaves `agents.toml` unchanged and keeps the lock row (the
/// stub does both), then run `dotagents install`, then turn the skill on.
/// Expectation: park adds `foo` to that list itself (keeping the comment and
/// `baz`), drops the lock row, install leaves the skill parked, and turn-on
/// lifts only `foo`. Failure: park fails or reports success while the next
/// install brings the copy back.
#[test]
fn park_handles_a_wildcard_entry_with_a_multi_line_exclude() {
    let (home, stub) = dotagents_home("park_dotagents_multiline", MULTILINE_EXCLUDE_TOML);
    let rt = runtime(&home, stub.clone(), true);
    stub.keep_lock_row.store(true, Ordering::SeqCst);

    park_foo(&rt);

    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("# setup") && toml.contains("\"baz\"") && toml.contains("\"foo\""),
        "{toml}"
    );
    assert!(!read(&stub.lock_path()).contains("[skills.foo]"));
    dotagents_install(&stub);
    assert!(
        !home.join(".agents/skills/foo").exists(),
        "dotagents install brought the parked skill back"
    );

    unpark_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("# setup") && toml.contains("\"baz\"") && !toml.contains("\"foo\""),
        "{toml}"
    );
    assert!(read(&stub.lock_path()).contains("[skills.foo]"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn on a parked dotagents skill when `agents.lock` is gone, and
/// when it exists with a `version` line. Expectation: a missing lock stays
/// missing, an existing one keeps `version = 1` first. Failure: turn-on
/// creates a lock without a version, which breaks every dotagents command.
#[test]
fn turn_on_never_creates_agents_lock_and_keeps_its_version_line() {
    let (home, stub) = dotagents_home("park_dotagents_lock_missing", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    std::fs::remove_file(stub.lock_path()).unwrap();
    unpark_foo(&rt);
    assert!(!stub.lock_path().exists(), "turn-on created agents.lock");
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();

    let home = unique_temp_dir("park_dotagents_lock_version");
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(home.join(".agents/agents.toml"), WILDCARD_TOML).unwrap();
    std::fs::write(home.join(".agents/agents.lock"), "version = 1\n").unwrap();
    let stub = FakeDotagents::new(&home, &["foo", "bar"]);
    dotagents_install(&stub);
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    unpark_foo(&rt);
    let lock = read(&stub.lock_path());
    assert!(lock.starts_with("version = 1\n"), "{lock}");
    assert!(lock.contains("[skills.foo]"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a project skill when the project folder is a link into a
/// folder inside a bigger git repository. Expectation: park is refused with
/// the git root, as for the real path. Failure: the link hides the
/// repository, and dotagents edits the repository's `agents.toml`.
#[cfg(unix)]
#[test]
fn project_park_through_a_link_into_a_larger_repository_is_refused() {
    let home = unique_temp_dir("park_dotagents_linked");
    let repo = home.join("repo");
    let project = repo.join("packages/app");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    project_with_foo(&project);
    let link = home.join("linked-app");
    std::os::unix::fs::symlink(&project, &link).unwrap();
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime_with_projects(&home, stub.clone(), true, vec![link.clone()]);
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
    assert!(stub.calls().is_empty());
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

/// An own entry for `foo` and a `name = "*"` entry of the same source.
const OWN_AND_WILDCARD_TOML: &str = "version = 1\n\n[[skills]]\nname = \"foo\"\nsource = \"owner/pack\"\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n";
/// A comment above the last `exclude` item and one after it.
const COMMENTED_EXCLUDE_TOML: &str = "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\nexclude = [\n  \"a\",\n  # note\n  \"baz\", # tail\n]\n";
/// Two `name = "*"` entries of one source with different `path`s.
const TWO_PATHS_TOML: &str = "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\npath = \"a\"\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\npath = \"b\"\n";

/// Flow: `foo` has its own entry and the `*` entry of the same source also
/// supplies it. Park, run `dotagents install`, turn on. Expectation: park
/// removes the own entry and excludes `foo` in the `*` entry, install leaves
/// the skill parked, turn-on brings back the entry and lifts the exclude.
/// Failure: install revives the skill through the `*` entry.
#[test]
fn a_skill_with_an_own_entry_and_a_wildcard_entry_stays_parked() {
    let (home, stub) = dotagents_home_with_pool(
        "park_dotagents_own_and_wildcard",
        OWN_AND_WILDCARD_TOML,
        &["foo", "bar"],
    );
    let rt = runtime(&home, stub.clone(), true);
    let live = home.join(".agents/skills/foo");

    park_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(
        !toml.contains("name = \"foo\"") && toml.contains("exclude = [\"foo\"]"),
        "{toml}"
    );
    dotagents_install(&stub);
    assert!(!live.exists(), "install revived the parked skill");

    unpark_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("name = \"foo\"") && !toml.contains("exclude"),
        "{toml}"
    );
    std::fs::remove_dir_all(&live).unwrap();
    dotagents_install(&stub);
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: real dotagents leaves `agents.toml` unchanged for a CRLF file. The
/// stub does the same. Expectation: park adds `foo` to `exclude` itself and
/// install leaves the skill parked. Failure: park fails or the skill returns.
#[test]
fn park_repairs_a_crlf_agents_toml_that_dotagents_left_unchanged() {
    let crlf = WILDCARD_TOML.replace('\n', "\r\n");
    let (home, stub) = dotagents_home("park_dotagents_crlf", &crlf);
    let rt = runtime(&home, stub.clone(), true);
    stub.leave_toml.store(true, Ordering::SeqCst);

    park_foo(&rt);

    let parked = read(&stub.toml_path());
    assert!(parked.contains("exclude = [\"foo\"]"));
    assert_eq!(
        parked.replace("\r\n", "").matches('\n').count(),
        0,
        "park wrote a bare LF into a CRLF file"
    );
    dotagents_install(&stub);
    assert!(
        !home.join(".agents/skills/foo").exists(),
        "install revived the parked skill"
    );

    unpark_foo(&rt);
    assert_eq!(read(&stub.toml_path()), crlf, "turn-on must keep CRLF");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: two `*` entries share a source; only the one with `path = "b"`
/// supplies `foo`, but dotagents excludes `foo` in the first one. Expectation:
/// park excludes `foo` in the entry that supplies it. Failure: the skill is
/// still supplied and the next install brings it back.
#[test]
fn park_excludes_the_wildcard_entry_that_really_supplies_the_skill() {
    let (home, stub) =
        dotagents_home_with_pool("park_dotagents_two_paths", TWO_PATHS_TOML, &["foo"]);
    let lock = read(&stub.lock_path()).replace(
        "[skills.foo]\nsource = \"owner/pack\"\n",
        "[skills.foo]\nsource = \"owner/pack\"\nresolved_path = \"b\"\n",
    );
    std::fs::write(stub.lock_path(), lock).unwrap();
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);

    let doc = read(&stub.toml_path())
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    let rows = doc["skills"].as_array_of_tables().unwrap();
    let excludes_foo = |row: &toml_edit::Table| {
        row.get("exclude")
            .and_then(toml_edit::Item::as_array)
            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some("foo")))
    };
    assert!(rows.iter().all(excludes_foo), "{doc}");
    dotagents_install(&stub);
    assert!(!home.join(".agents/skills/foo").exists());

    unpark_foo(&rt);
    assert_eq!(
        read(&stub.toml_path()),
        TWO_PATHS_TOML,
        "turn-on must lift the exclude from both rows"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill whose `*` entry has a multi-line `exclude` with a
/// comment above the last item and one after it, with and without a trailing
/// comma, then turn it on. Expectation: `foo` goes in as a new last item and
/// both comments stay once, where they were; turn-on gives back the original
/// bytes. Failure: a comment is copied, moves, or is lost on turn-on.
#[test]
fn park_and_turn_on_keep_the_comments_around_the_last_exclude_item() {
    let no_comma = COMMENTED_EXCLUDE_TOML.replace("\"baz\", # tail\n", "\"baz\" # tail\n");
    let forms = [
        (
            COMMENTED_EXCLUDE_TOML.to_string(),
            "\"baz\", # tail\n  \"foo\",\n]\n",
        ),
        (no_comma, "\"baz\", # tail\n  \"foo\"\n]\n"),
    ];
    for (toml, parked_tail) in forms {
        let (home, stub) = dotagents_home("park_dotagents_comments", &toml);
        let rt = runtime(&home, stub.clone(), true);
        stub.keep_lock_row.store(true, Ordering::SeqCst);

        park_foo(&rt);
        let parked = read(&stub.toml_path());
        assert!(
            parked.ends_with(&format!("  # note\n  {parked_tail}")),
            "{parked}"
        );

        unpark_foo(&rt);
        assert_eq!(read(&stub.toml_path()), toml);
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: a no-comma `exclude` has a comment after the last item and a
/// commented-out item on the next line. Park, then turn on. Expectation: the
/// commented-out line stays through park and the file is byte for byte as it
/// was after turn-on. Failure: park deletes the comment line.
#[test]
fn park_keeps_comment_lines_after_the_last_exclude_item() {
    let toml = "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\nexclude = [\n  \"baz\" # tail\n  # \"old\"\n]\n";
    let (home, stub) = dotagents_home("park_dotagents_comment_lines", toml);
    let rt = runtime(&home, stub.clone(), true);
    stub.keep_lock_row.store(true, Ordering::SeqCst);

    park_foo(&rt);
    let parked = read(&stub.toml_path());
    assert!(parked.contains("  # \"old\"\n]"), "{parked}");

    unpark_foo(&rt);
    assert_eq!(read(&stub.toml_path()), toml);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park, then the person puts more items on the line after `foo`,
/// then turn on. Expectation: `foo` leaves the list, the other items stay
/// excluded, and the file still parses. Failure: the tail comment swallows
/// the next item or the closing bracket.
#[test]
fn turn_on_keeps_items_the_person_added_after_the_parked_name() {
    let (home, stub) = dotagents_home("park_dotagents_edited_list", COMMENTED_EXCLUDE_TOML);
    let rt = runtime(&home, stub.clone(), true);
    stub.keep_lock_row.store(true, Ordering::SeqCst);
    park_foo(&rt);
    let edited = read(&stub.toml_path()).replace("  \"foo\",\n]", "  \"foo\", \"new\",\n]");
    assert!(edited.contains("\"new\""));
    std::fs::write(stub.toml_path(), edited).unwrap();

    unpark_foo(&rt);

    let doc = read(&stub.toml_path())
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    let list: Vec<&str> = doc["skills"][0]["exclude"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item.as_str())
        .collect();
    assert_eq!(list, ["a", "baz", "new"]);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: two `*` entries share a source, dotagents excludes the first, and
/// the person deletes the `path = "b"` entry that supplies `foo` while it is
/// parked. Expectation: turn-on refuses and the copy stays parked. Failure:
/// turn-on counts the first entry, which never supplied `foo`, and reports
/// success for a skill dotagents will not install.
#[test]
fn turn_on_refuses_when_only_the_entry_dotagents_excluded_is_left() {
    let (home, stub) =
        dotagents_home_with_pool("park_dotagents_target_only", TWO_PATHS_TOML, &["foo"]);
    let lock = read(&stub.lock_path()).replace(
        "[skills.foo]\nsource = \"owner/pack\"\n",
        "[skills.foo]\nsource = \"owner/pack\"\nresolved_path = \"b\"\n",
    );
    std::fs::write(stub.lock_path(), lock).unwrap();
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    let mut doc = read(&stub.toml_path())
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    doc["skills"].as_array_of_tables_mut().unwrap().remove(1);
    std::fs::write(stub.toml_path(), doc.to_string()).unwrap();
    let toml_parked = read(&stub.toml_path());

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo").id,
        },
    );

    assert!(result.is_err());
    assert_eq!(read(&stub.toml_path()), toml_parked);
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    std::fs::remove_dir_all(&home).ok();
}

const ROW_IN_THE_MIDDLE_TOML: &str = "# my dotagents setup\nversion = 1\n\n# first\n[[skills]]\nname = \"bar\"\nsource = \"owner/bar\"\n\n# the one to park\n[[skills]]\nname = \"foo\"\nsource = \"owner/foo\"   # pinned\n\n# last\n[[skills]]\nname = \"baz\"\nsource = \"owner/baz\"\n";

const WILDCARD_EMPTY_EXCLUDE_TOML: &str = "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pool\"\nexclude = []   # nothing excluded yet\n";

/// The `.gitignore` dotagents keeps beside `agents.toml`: it lists the
/// installed skills, and `remove` deletes the skill's line.
fn write_gitignore(home: &Path) -> String {
    let text = "# dotagents\n/skills/bar\n/skills/foo\n/skills/baz\n".to_string();
    std::fs::write(home.join(".agents/.gitignore"), &text).unwrap();
    text
}

fn lock_with_rows(names: &[&str]) -> String {
    names
        .iter()
        .fold("version = 1\n".to_string(), |text, name| {
            text + "\n[skills." + name + "]\nsource = \"owner/" + name + "\"\n"
        })
}

/// Flow: park an explicit `[[skills]]` row that sits in the middle of
/// `agents.toml` between commented rows, with `agents.lock` rows and a
/// `.gitignore` line, then turn it on and nothing else touched the files.
/// Expectation: all three files are byte for byte what they were before the
/// park. Failure: the row comes back at the end of the file, the comments
/// dotagents deleted stay lost, the lock row order changes, or the skill
/// shows as untracked in git.
#[test]
fn turn_on_of_an_explicit_row_in_the_middle_gives_all_three_files_back_byte_for_byte() {
    let (home, stub) = dotagents_home("park_dotagents_exact_explicit", ROW_IN_THE_MIDDLE_TOML);
    std::fs::write(stub.lock_path(), lock_with_rows(&["bar", "foo", "baz"])).unwrap();
    let gitignore = write_gitignore(&home);
    let toml = read(&stub.toml_path());
    let lock = read(&stub.lock_path());
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    assert_ne!(
        read(&stub.toml_path()),
        toml,
        "the stub's remove changed nothing"
    );
    assert!(!read(&home.join(".agents/.gitignore")).contains("/skills/foo"));

    unpark_foo(&rt);
    assert_eq!(read(&stub.toml_path()), toml, "agents.toml");
    assert_eq!(read(&stub.lock_path()), lock, "agents.lock");
    assert_eq!(
        read(&home.join(".agents/.gitignore")),
        gitignore,
        ".gitignore"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park the only skill in `agents.toml` and `agents.lock`, so dotagents
/// leaves no `skills` array and an empty `[skills]` table behind, then turn it
/// on with nothing else changed.
/// Expectation: `agents.lock` is byte for byte what it was before the park.
/// Failure: the file keeps a bare `[skills]` header and a blank line before
/// `[skills.foo]` (#419).
#[test]
fn turn_on_of_the_only_locked_skill_gives_agents_lock_back_byte_for_byte() {
    let (home, stub) =
        dotagents_home_with_pool("park_dotagents_only_lock_row", ONLY_FOO_TOML, &["foo"]);
    std::fs::write(stub.lock_path(), lock_with_rows(&["foo"])).unwrap();
    let lock = read(&stub.lock_path());
    let toml = read(&stub.toml_path());
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    assert!(
        read(&stub.lock_path()).contains("[skills]"),
        "the stub's remove left no empty [skills] table"
    );

    unpark_foo(&rt);
    assert_eq!(read(&stub.lock_path()), lock, "agents.lock");
    assert_eq!(read(&stub.toml_path()), toml, "agents.toml");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a skill parked before install-aware park is still listed in
/// `agents.toml` and `agents.lock`; the repair runs `dotagents remove -y`.
/// Expectation: `agents.toml` no longer lists it and the parked folder is
/// untouched. Failure: the entry stays, so the next `dotagents install`
/// brings the skill back, or the repair deletes the parked copy (#402).
#[test]
fn unlisting_a_parked_skill_still_in_agents_toml_drops_the_entry_and_keeps_the_parked_copy() {
    let (home, stub) = dotagents_home("park_dotagents_unlist", EXPLICIT_TOML);
    let toml = read(&stub.toml_path());
    let lock = read(&stub.lock_path());
    let rt = runtime(&home, stub.clone(), true);
    park_foo(&rt);
    std::fs::write(stub.toml_path(), &toml).unwrap();
    std::fs::write(stub.lock_path(), &lock).unwrap();
    let removes_before = stub.removes();
    let parked = parked_copy(&rt, "foo");
    assert_eq!(
        parked.owner_kind,
        skill_studio_core::identity::LifecycleOwnerKind::Dotagents,
        "the pre-park-aware state is not read as dotagents-owned"
    );

    ops::unlist_parked_dotagents(&rt, &ctx(), &parked.id).unwrap();

    assert_eq!(stub.removes(), removes_before + 1);
    assert!(!read(&stub.toml_path()).contains("name = \"foo\""));
    assert!(read(&stub.toml_path()).contains("name = \"bar\""));
    assert!(parked.path.join("SKILL.md").exists(), "parked copy lost");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill supplied by a `*` row whose `exclude = []   # comment`
/// line is written by hand, then turn it on with nothing else changed.
/// Expectation: the original line is back, comment included. Failure: the
/// empty `exclude` key is deleted with its comment.
#[test]
fn turn_on_of_a_wildcard_row_keeps_its_empty_exclude_line_and_comment() {
    let (home, stub) = dotagents_home_with_pool(
        "park_dotagents_exact_wildcard",
        WILDCARD_EMPTY_EXCLUDE_TOML,
        &["foo", "bar"],
    );
    std::fs::write(
        stub.lock_path(),
        lock_with_rows(&["foo", "bar"])
            .replace("owner/foo", "owner/pool")
            .replace("owner/bar", "owner/pool"),
    )
    .unwrap();
    let toml = read(&stub.toml_path());
    let lock = read(&stub.lock_path());
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    assert!(read(&stub.toml_path()).contains("\"foo\""));

    unpark_foo(&rt);
    assert_eq!(read(&stub.toml_path()), toml);
    assert_eq!(read(&stub.lock_path()), lock);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: as the wildcard flow above, but `agents.toml` gains another row
/// after the park, so the post-park hash no longer matches. Expectation: the
/// edit path runs: the `exclude` key stays with its comment even though it
/// is empty again, and the new row stays. Failure: the key is deleted, or
/// the new row is lost.
#[test]
fn turn_on_after_an_edit_keeps_the_original_empty_exclude_key() {
    let (home, stub) = dotagents_home_with_pool(
        "park_dotagents_edit_wildcard",
        WILDCARD_EMPTY_EXCLUDE_TOML,
        &["foo", "bar"],
    );
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let mut toml = read(&stub.toml_path());
    toml.push_str("\n[[skills]]\nname = \"extra\"\nsource = \"owner/extra\"\n");
    std::fs::write(stub.toml_path(), toml).unwrap();

    unpark_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("exclude = []   # nothing excluded yet"),
        "{toml}"
    );
    assert!(toml.contains("name = \"extra\""), "{toml}");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park an explicit skill, then add another row to `agents.toml`
/// and delete the skill's `.gitignore` line again, then turn it on.
/// Expectation: the new row stays, the skill row is listed again, and the
/// `.gitignore` line is back. The untouched `agents.lock` still returns
/// byte for byte. Failure: the new row is overwritten by the backup, or the
/// skill stays out of `agents.toml`.
#[test]
fn turn_on_after_agents_toml_was_edited_keeps_the_new_row_and_lists_the_skill_again() {
    let (home, stub) = dotagents_home("park_dotagents_edit_explicit", ROW_IN_THE_MIDDLE_TOML);
    std::fs::write(stub.lock_path(), lock_with_rows(&["bar", "foo", "baz"])).unwrap();
    let gitignore = write_gitignore(&home);
    let lock = read(&stub.lock_path());
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let mut toml = read(&stub.toml_path());
    toml.push_str("\n[[skills]]\nname = \"extra\"\nsource = \"owner/extra\"\n");
    std::fs::write(stub.toml_path(), toml).unwrap();
    std::fs::write(
        home.join(".agents/.gitignore"),
        "# dotagents\n/skills/bar\n",
    )
    .unwrap();

    unpark_foo(&rt);
    let toml = read(&stub.toml_path());
    assert!(toml.contains("name = \"extra\""), "{toml}");
    assert!(
        toml.contains("name = \"foo\"") && toml.contains("source = \"owner/foo\""),
        "{toml}"
    );
    assert_eq!(read(&stub.lock_path()), lock, "agents.lock");
    let restored = read(&home.join(".agents/.gitignore"));
    assert!(restored.contains("/skills/foo"), "{restored}");
    assert!(gitignore.contains("/skills/foo"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: `dotagents remove` exits non-zero and the rollback cannot move the
/// copy back, because a folder now sits at the skill's own path. Then that
/// folder goes away and the person turns the skill on. Expectation: the
/// failed row is still offered, so turn-on moves the copy back and the skill
/// is listed in `agents.toml` and `agents.lock` as before. Failure: the row
/// is dropped as failed, and turn-on restores the folder without the
/// recorded entries.
#[test]
fn a_failed_remove_whose_rollback_also_failed_can_still_be_turned_on() {
    let (home, stub) = dotagents_home("park_dotagents_rollback_fails", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let (toml, lock) = (read(&stub.toml_path()), read(&stub.lock_path()));
    stub.remove_exit.store(3, Ordering::SeqCst);
    stub.block_move_back.store(true, Ordering::SeqCst);

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    )
    .unwrap_err();
    assert!(
        err.message.contains("could not be moved back"),
        "{}",
        err.message
    );
    assert_eq!(statuses(&rt), ["failed"]);
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());

    std::fs::remove_dir_all(home.join(".agents/skills/foo")).unwrap();
    // The files as a half-done park could leave them: no entry for foo.
    std::fs::write(stub.toml_path(), "version = 1\n").unwrap();
    std::fs::write(stub.lock_path(), "version = 1\n").unwrap();
    unpark_foo(&rt);

    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    let (turned_on_toml, turned_on_lock) = (read(&stub.toml_path()), read(&stub.lock_path()));
    assert!(
        turned_on_toml.contains("name = \"foo\""),
        "{turned_on_toml}"
    );
    assert!(turned_on_lock.contains("[skills.foo]"), "{turned_on_lock}");
    assert!(toml.contains("name = \"foo\"") && lock.contains("[skills.foo]"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park and turn on a wildcard skill, the person changes the wildcard
/// source, then a second park fails and its rollback fails too. Expectation:
/// turn-on uses the second park's record, so the skill comes back under the
/// new source. Failure: turn-on picks the first, finished park row, lists the
/// old source, and the skill stays parked.
#[test]
fn turn_on_after_a_failed_second_park_uses_the_newest_park_record() {
    let (home, stub) = dotagents_home("park_dotagents_newest_row", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    unpark_foo(&rt);
    let new_source = WILDCARD_TOML.replace("owner/pack", "owner/other");
    std::fs::write(stub.toml_path(), &new_source).unwrap();
    let lock = read(&stub.lock_path()).replace("owner/pack", "owner/other");
    std::fs::write(stub.lock_path(), lock).unwrap();

    stub.remove_exit.store(3, Ordering::SeqCst);
    stub.block_move_back.store(true, Ordering::SeqCst);
    let second = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    );
    assert!(second.is_err());
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());

    std::fs::remove_dir_all(home.join(".agents/skills/foo")).unwrap();
    unpark_foo(&rt);

    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    let toml = read(&stub.toml_path());
    assert!(
        toml.contains("owner/other") && !toml.contains("owner/pack"),
        "{toml}"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: `dotagents remove` exits non-zero and the rollback works. The copy
/// is then moved into the parked folder by hand and turned on. Expectation:
/// the failed row is not offered, so turn-on does not rewrite `agents.toml`
/// from it. Failure: a failed park's recorded files overwrite what the
/// person has now.
#[test]
fn a_failed_row_with_a_completed_rollback_is_not_offered_to_turn_on() {
    let (home, stub) = dotagents_home("park_dotagents_failed_not_offered", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    stub.remove_exit.store(3, Ordering::SeqCst);
    let parked = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    );
    assert!(parked.is_err());
    assert_eq!(statuses(&rt), ["failed"]);

    let edited = format!("{}\n# edited by hand\n", read(&stub.toml_path()));
    std::fs::write(stub.toml_path(), &edited).unwrap();
    let slot = home.join(".agents/skills-parked/universal");
    std::fs::create_dir_all(&slot).unwrap();
    std::fs::rename(home.join(".agents/skills/foo"), slot.join("foo")).unwrap();
    unpark_foo(&rt);

    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert_eq!(read(&stub.toml_path()), edited);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: `.agents/.gitignore` is a link (absolute or relative) to a file
/// elsewhere; park an explicit skill, then turn it on. Expectation: the
/// target file holds its original bytes and the name is still a link.
/// Failure: the backup reads the link, turn-on replaces the link with a
/// plain file, or the target keeps the park's edit.
#[cfg(unix)]
#[test]
fn a_symlinked_ignore_file_comes_back_byte_for_byte_and_stays_a_link() {
    for absolute in [true, false] {
        let (home, stub) = dotagents_home("park_dotagents_ignore_link", EXPLICIT_TOML);
        let original = write_gitignore(&home);
        let store = home.join("dotfiles");
        std::fs::create_dir_all(&store).unwrap();
        let target = store.join("agents-ignore");
        let link = home.join(".agents/.gitignore");
        std::fs::rename(&link, &target).unwrap();
        let link_text = if absolute {
            target.clone()
        } else {
            PathBuf::from("../dotfiles/agents-ignore")
        };
        std::os::unix::fs::symlink(&link_text, &link).unwrap();
        let rt = runtime(&home, stub.clone(), true);

        park_foo(&rt);
        assert!(!read(&target).contains("/skills/foo"), "park left the line");
        unpark_foo(&rt);

        assert_eq!(read(&target), original, "absolute link: {absolute}");
        assert_eq!(std::fs::read_link(&link).unwrap(), link_text);
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: `.agents/.gitignore` is a link to a file outside the home and the
/// configured projects. Expectation: park refuses before anything moves or
/// dotagents runs. Failure: park succeeds and turn-on later fails at the
/// ignore file, leaving the skill parked.
#[cfg(unix)]
#[test]
fn an_ignore_file_link_leaving_the_scope_makes_park_refuse_and_nothing_moves() {
    let (home, stub) = dotagents_home("park_dotagents_ignore_outside", EXPLICIT_TOML);
    let outside = unique_temp_dir("park_dotagents_ignore_outside_target");
    std::fs::create_dir_all(&outside).unwrap();
    let target = outside.join("ignore");
    std::fs::write(&target, "/skills/foo\n").unwrap();
    std::os::unix::fs::symlink(&target, home.join(".agents/.gitignore")).unwrap();
    let rt = runtime(&home, stub.clone(), true);
    let calls_before = stub.calls().len();

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("outside"), "{}", err.message);
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert!(!home.join(".agents/skills-parked").exists());
    assert_eq!(stub.calls().len(), calls_before);
    assert_eq!(read(&target), "/skills/foo\n");
    assert!(statuses(&rt).is_empty());
    std::fs::remove_dir_all(&home).ok();
    std::fs::remove_dir_all(&outside).ok();
}

/// Flow: `.agents/.gitignore` has `/skills/foo` then `!/skills/foo/`. Park
/// removes the first line, the person adds a comment, then turn on. Expectation:
/// the line returns before the negation and the comment stays. Failure: it is
/// appended after the negation and git ignores a folder that was visible.
#[test]
fn turn_on_puts_the_ignore_line_back_before_the_negation_that_followed_it() {
    let (home, stub) = dotagents_home("park_dotagents_ignore_order", EXPLICIT_TOML);
    let ignore = home.join(".agents/.gitignore");
    std::fs::write(&ignore, "# dotagents\n/skills/foo\n!/skills/foo/\n").unwrap();
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    std::fs::write(&ignore, read(&ignore) + "# mine\n").unwrap();
    unpark_foo(&rt);

    assert_eq!(
        read(&ignore),
        "# dotagents\n/skills/foo\n!/skills/foo/\n# mine\n"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the ignore file repeats a line (`# group`) around the skill's rule.
/// Park removes the rule, the person adds a comment, then turn on.
/// Expectation: the rule returns after the negation and before the second
/// `# group`. Failure: the repeated line is matched by text, the rule lands
/// before the negation, and git shows a folder that was ignored.
#[test]
fn turn_on_restores_the_ignore_line_beside_the_right_repeat_of_a_line() {
    let (home, stub) = dotagents_home("park_dotagents_ignore_repeat", EXPLICIT_TOML);
    let ignore = home.join(".agents/.gitignore");
    std::fs::write(&ignore, "# group\n!/skills/foo/\n/skills/foo\n# group\n").unwrap();
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    std::fs::write(&ignore, read(&ignore) + "# mine\n").unwrap();
    unpark_foo(&rt);

    assert_eq!(
        read(&ignore),
        "# group\n!/skills/foo/\n/skills/foo\n# group\n# mine\n"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the ignore file has the skill's rule on both sides of a negation.
/// Park removes both, the person adds a comment, then turn on. Expectation:
/// both rules return around the negation and the comment stays. Failure: only
/// the first returns and the folder shows up in git.
#[test]
fn turn_on_restores_every_removed_copy_of_the_ignore_rule() {
    let (home, stub) = dotagents_home("park_dotagents_ignore_twice", EXPLICIT_TOML);
    let ignore = home.join(".agents/.gitignore");
    std::fs::write(&ignore, "/skills/foo\n!/skills/foo/\n/skills/foo\n").unwrap();
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    std::fs::write(&ignore, read(&ignore) + "# mine\n").unwrap();
    unpark_foo(&rt);

    assert_eq!(
        read(&ignore),
        "/skills/foo\n!/skills/foo/\n/skills/foo\n# mine\n"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a 50k-line ignore file is rewritten almost entirely after the park,
/// then turn on. Expectation: turn-on refuses with a clear error and nothing
/// changes: the skill stays parked and `agents.toml` and `agents.lock` are as
/// the park left them. Failure: the alignment runs out of memory after the
/// config files were already written.
#[test]
fn turn_on_refuses_an_ignore_file_too_changed_to_merge_and_changes_nothing() {
    let (home, stub) = dotagents_home("park_dotagents_ignore_huge", EXPLICIT_TOML);
    let ignore = home.join(".agents/.gitignore");
    let others: Vec<String> = (0..50_000).map(|i| format!("/skills/other-{i}")).collect();
    std::fs::write(&ignore, format!("/skills/foo\n{}\n", others.join("\n"))).unwrap();
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let changed: Vec<String> = (0..50_000).map(|i| format!("# changed {i}")).collect();
    let rewritten = changed.join("\n") + "\n";
    std::fs::write(&ignore, &rewritten).unwrap();
    let (toml, lock) = (read(&stub.toml_path()), read(&stub.lock_path()));
    let err = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo").id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("too much to merge"), "{}", err.message);
    assert_eq!(read(&stub.toml_path()), toml);
    assert_eq!(read(&stub.lock_path()), lock);
    assert_eq!(read(&ignore), rewritten);
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    assert!(!home.join(".agents/skills/foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park and turn on a skill in a project whose `.agents/.gitignore`
/// lists it. Expectation: the project's file is byte for byte what it was.
/// Failure: the project scope reads the file at the wrong path and leaves
/// the removed line out.
#[test]
fn project_turn_on_gives_the_ignore_file_back_byte_for_byte() {
    let home = unique_temp_dir("park_dotagents_project_ignore");
    let project = home.join("proj");
    project_with_foo(&project);
    let original = "# dotagents\n/skills/foo\n";
    std::fs::write(project.join(".agents/.gitignore"), original).unwrap();
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime_with_projects(&home, stub.clone(), true, vec![project.clone()]);

    ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_project_copy(&rt, "foo").id,
        },
    )
    .unwrap();
    assert!(!read(&project.join(".agents/.gitignore")).contains("/skills/foo"));
    unpark_foo(&rt);

    assert_eq!(read(&project.join(".agents/.gitignore")), original);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: while `dotagents remove` runs, another tool adds an unrelated skill
/// to `agents.toml`; then the skill is turned on. Expectation: the unrelated
/// entry is still listed and so is the parked skill. Failure: Turn on
/// restores the backup it took before the park and drops the other tool's
/// entry.
#[test]
fn turn_on_keeps_an_entry_another_tool_added_while_dotagents_ran() {
    let (home, stub) = dotagents_home("park_dotagents_concurrent_edit", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    stub.add_unrelated_entry.store(true, Ordering::SeqCst);

    park_foo(&rt);
    unpark_foo(&rt);

    let toml = read(&stub.toml_path());
    assert!(toml.contains("name = \"extra\""), "{toml}");
    assert!(toml.contains("name = \"foo\""), "{toml}");
    std::fs::remove_dir_all(&home).ok();
}
