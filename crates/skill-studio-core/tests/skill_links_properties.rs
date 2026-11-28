// Integration test crate: `unwrap` and `expect` are the assertions here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Skill links agree with what each agent can actually reach, after any
//! sequence of real-world actions (`docs/spec-skill-links.md` section 10,
//! layer 2). A random sequence of installs, removals, parks, renames, edits
//! and deletes runs on a real temp home in one of three folder layouts.
//! After every step an oracle that knows no Skill Studio rule, only the file
//! system, says whether each agent can open each path and find each name;
//! `skill_links` must say the same. A failure shrinks to its shortest case.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use proptest::prelude::*;
use skill_studio_core::frontmatter::frontmatter_fields;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::ops_skill_links::{skill_links, LinkState, SkillLinksRequest};
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::skill_links::LinkStrength;
use skill_studio_core::testing::golden::ctx;
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};
use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const SHARED: &str = ".agents/skills";
const PARKED: &str = ".agents/skills-parked";
const CLAUDE: &str = ".claude/skills";
const AGENTS: [&str; 2] = ["claude-code", "codex"];
/// Real skills that run, read or are needed by others, so random steps
/// often split a linked pair between agents.
const LINKED: [&str; 12] = [
    "grill-me",
    "grilling",
    "grill-with-docs",
    "domain-modeling",
    "triage",
    "setup-matt-pocock-skills",
    "to-spec",
    "chief-of-staff",
    "implement-spec",
    "find-animation-opportunities",
    "choreograph",
    "review-animations",
];

#[derive(Debug, Clone, Copy)]
enum Layout {
    /// A per-skill link in Claude Code's folder to the shared copy.
    Links,
    /// Claude Code's whole folder is a link to the shared folder.
    Whole,
    /// A separate copy in Claude Code's folder.
    Copies,
}

fn real_skills() -> Vec<(String, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/real-packs");
    let mut out = Vec::new();
    for pack in ["mattpocock_skills", "emilkowalski_skills"] {
        let mut stack = vec![root.join(pack).join("skills")];
        while let Some(dir) = stack.pop() {
            if dir.join("SKILL.md").is_file() {
                out.push((dir.file_name().unwrap().to_string_lossy().to_string(), dir));
                continue;
            }
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn remove_entry(path: &Path) {
    if path.is_symlink() || path.is_file() {
        fs::remove_file(path).unwrap();
    } else if path.is_dir() {
        fs::remove_dir_all(path).unwrap();
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.retain(|n| !n.starts_with('.'));
    names.sort();
    names
}

struct World {
    home: tempfile::TempDir,
    layout: Layout,
}

impl World {
    fn new(layout: Layout) -> Self {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(SHARED)).unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        match layout {
            Layout::Whole => {
                std::os::unix::fs::symlink(home.path().join(SHARED), home.path().join(CLAUDE))
                    .unwrap();
            }
            Layout::Links | Layout::Copies => fs::create_dir_all(home.path().join(CLAUDE)).unwrap(),
        }
        World { home, layout }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.home.path().join(rel)
    }

    /// Done the way skills.sh does it: a shared copy, then Claude Code's entry.
    fn install(&self, name: &str, source: &Path) {
        let shared = self.path(SHARED).join(name);
        if shared.exists() {
            return;
        }
        copy_dir(source, &shared);
        let claude = self.path(CLAUDE).join(name);
        match self.layout {
            Layout::Links => {
                remove_entry(&claude);
                std::os::unix::fs::symlink(&shared, &claude).unwrap();
            }
            Layout::Copies => {
                remove_entry(&claude);
                copy_dir(&shared, &claude);
            }
            Layout::Whole => {}
        }
    }

    /// An install for one agent: a copy in Claude Code's folder only, or in
    /// the shared folder only (what an install for Codex alone leaves).
    fn install_for_one(&self, name: &str, source: &Path, claude: bool) {
        let target = self.path(if claude { CLAUDE } else { SHARED }).join(name);
        if !target.exists() && !target.is_symlink() {
            copy_dir(source, &target);
        }
    }

    fn remove(&self, name: &str) {
        remove_entry(&self.path(CLAUDE).join(name));
        remove_entry(&self.path(SHARED).join(name));
    }

    fn park(&self, name: &str) {
        fs::create_dir_all(self.path(PARKED)).unwrap();
        remove_entry(&self.path(PARKED).join(name));
        if !matches!(self.layout, Layout::Whole) {
            remove_entry(&self.path(CLAUDE).join(name));
        }
        fs::rename(self.path(SHARED).join(name), self.path(PARKED).join(name)).unwrap();
    }

    fn rename(&self, name: &str, new: &str) {
        let shared = self.path(SHARED).join(name);
        let skill_md = shared.join("SKILL.md");
        let text = fs::read_to_string(&skill_md).unwrap();
        fs::write(
            &skill_md,
            text.replacen(&format!("name: {name}"), &format!("name: {new}"), 1),
        )
        .unwrap();
        fs::rename(&shared, self.path(SHARED).join(new)).unwrap();
        let claude = self.path(CLAUDE).join(name);
        match self.layout {
            Layout::Links => {
                remove_entry(&claude);
                std::os::unix::fs::symlink(
                    self.path(SHARED).join(new),
                    self.path(CLAUDE).join(new),
                )
                .unwrap();
            }
            Layout::Copies => {
                remove_entry(&claude);
                copy_dir(&self.path(SHARED).join(new), &self.path(CLAUDE).join(new));
            }
            Layout::Whole => {}
        }
    }

    fn append(&self, name: &str, line: &str) {
        for root in [SHARED, CLAUDE] {
            let skill_md = self.path(root).join(name).join("SKILL.md");
            if (root != CLAUDE || matches!(self.layout, Layout::Copies)) && skill_md.is_file() {
                let text = fs::read_to_string(&skill_md).unwrap();
                fs::write(&skill_md, format!("{text}\n{line}\n")).unwrap();
            }
        }
    }
}

fn runtime(home: &Path) -> Runtime {
    let scope = RuntimeScope::fixture(home);
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
        scan_memo: None,
    };
    Runtime::new(&scope, ports).unwrap()
}

/// The oracle: the skills an agent finds by listing its own folder, by the
/// name in each `SKILL.md`, and where it finds them.
fn listing(world: &World, agent: &str) -> Vec<(String, PathBuf)> {
    let root = world.path(if agent == "claude-code" {
        CLAUDE
    } else {
        SHARED
    });
    entries(&root)
        .into_iter()
        .filter_map(|entry| {
            let dir = root.join(&entry);
            let text = fs::read_to_string(dir.join("SKILL.md")).ok()?;
            Some((
                frontmatter_fields(&text)
                    .get("name")
                    .cloned()
                    .unwrap_or(entry),
                dir,
            ))
        })
        .collect()
}

#[derive(Debug, Clone)]
enum Step {
    Install(usize),
    InstallForOne(bool, usize),
    InstallPack,
    InstallPackForOne(bool),
    Remove(usize),
    Park(usize),
    Rename(usize),
    AddSkillToolCall(usize, usize),
    AddPathRead(usize, usize),
    DeleteOutside(bool, usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => any::<usize>().prop_map(Step::Install),
        3 => (any::<bool>(), any::<usize>()).prop_map(|(c, i)| Step::InstallForOne(c, i)),
        1 => Just(Step::InstallPack),
        1 => any::<bool>().prop_map(Step::InstallPackForOne),
        2 => any::<usize>().prop_map(Step::Remove),
        1 => any::<usize>().prop_map(Step::Park),
        1 => any::<usize>().prop_map(Step::Rename),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::AddSkillToolCall(a, b)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::AddPathRead(a, b)),
        2 => (any::<bool>(), any::<usize>()).prop_map(|(c, a)| Step::DeleteOutside(c, a)),
    ]
}

fn apply(world: &World, catalog: &[(String, PathBuf)], step: &Step) {
    let installed = entries(&world.path(SHARED));
    let pick = |i: usize| installed.get(i % installed.len().max(1)).cloned();
    let any_name = |i: usize| LINKED[i % LINKED.len()].to_string();
    let linked = |i: usize| {
        let name = LINKED[i % LINKED.len()];
        catalog.iter().find(|(n, _)| n == name).unwrap()
    };
    match step {
        Step::Install(i) => {
            let (name, dir) = linked(*i);
            world.install(name, dir);
        }
        Step::InstallForOne(claude, i) => {
            let (name, dir) = linked(*i);
            world.install_for_one(name, dir, *claude);
        }
        Step::InstallPackForOne(claude) => {
            for (name, dir) in catalog
                .iter()
                .filter(|(_, d)| d.to_string_lossy().contains("mattpocock"))
            {
                world.install_for_one(name, dir, *claude);
            }
        }
        Step::InstallPack => {
            for (name, dir) in catalog
                .iter()
                .filter(|(_, d)| d.to_string_lossy().contains("mattpocock"))
            {
                world.install(name, dir);
            }
        }
        Step::Remove(i) => pick(*i).into_iter().for_each(|n| world.remove(&n)),
        Step::Park(i) => pick(*i).into_iter().for_each(|n| world.park(&n)),
        Step::Rename(i) => {
            if let Some(n) = pick(*i).filter(|n| !n.ends_with("-fork")) {
                world.rename(&n, &format!("{n}-fork"));
            }
        }
        Step::AddSkillToolCall(i, j) => {
            if let Some(n) = pick(*i) {
                world.append(
                    &n,
                    &format!("Call the Skill tool with \"{}\".", any_name(*j)),
                );
            }
        }
        Step::AddPathRead(i, j) => {
            if let Some(n) = pick(*i) {
                world.append(&n, &format!("First read ../{}/SKILL.md.", any_name(*j)));
            }
        }
        Step::DeleteOutside(claude, i) => {
            let root = world.path(if *claude { CLAUDE } else { SHARED });
            if let Some(n) = entries(&root).get(i % entries(&root).len().max(1)) {
                remove_entry(&root.join(n));
            }
        }
    }
}

/// Strong links a person checked by hand in the real packs
/// (`skill_links_corpus.rs`), as `(from, to, path)`. The oracle uses these
/// and the lines a step appends, never what `skill_links` found.
const KNOWN: [(&str, &str, Option<&str>); 13] = [
    ("grill-me", "grilling", None),
    ("grill-with-docs", "grilling", None),
    ("grill-with-docs", "domain-modeling", None),
    ("triage", "grilling", None),
    ("triage", "domain-modeling", None),
    ("triage", "setup-matt-pocock-skills", None),
    ("to-spec", "setup-matt-pocock-skills", None),
    (
        "chief-of-staff",
        "implement-spec",
        Some("../implement-spec/SKILL.md"),
    ),
    ("chief-of-staff", "retro", Some("../retro/SKILL.md")),
    ("chief-of-staff", "to-spec", Some("../to-spec/SKILL.md")),
    (
        "chief-of-staff",
        "to-tickets",
        Some("../to-tickets/SKILL.md"),
    ),
    (
        "find-animation-opportunities",
        "choreograph",
        Some("../choreograph/SKILL.md"),
    ),
    (
        "review-animations",
        "choreograph",
        Some("../choreograph/SKILL.md"),
    ),
];

/// The links the oracle expects from one copy: the known ones for the skill
/// it came from, plus the exact lines `apply` appended to this copy.
fn expected_links(name: &str, dir: &Path) -> BTreeMap<String, Option<String>> {
    let base = name.strip_suffix("-fork").unwrap_or(name);
    let mut out = BTreeMap::new();
    for (from, to, path) in KNOWN {
        if from == base && to != name {
            out.insert(to.to_string(), path.map(str::to_string));
        }
    }
    let text = fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
    for line in text.lines() {
        if let Some(to) = line
            .strip_prefix("Call the Skill tool with \"")
            .and_then(|r| r.strip_suffix("\"."))
        {
            if to != name {
                out.entry(to.to_string()).or_insert(None);
            }
        }
        if let Some(to) = line
            .strip_prefix("First read ../")
            .and_then(|r| r.strip_suffix("/SKILL.md."))
        {
            if to != name && to != dir.file_name().unwrap().to_string_lossy() {
                out.insert(to.to_string(), Some(format!("../{to}/SKILL.md")));
            }
        }
    }
    out
}

/// What the file system says about one link for one agent.
fn oracle_state(
    world: &World,
    found: &[(String, PathBuf)],
    dir: &Path,
    to: &str,
    path: Option<&str>,
) -> LinkState {
    let reachable = match path {
        Some(path) => dir.join(path).exists(),
        None => found.iter().any(|(n, _)| n == to),
    };
    if reachable {
        return LinkState::Ok;
    }
    let named = |root: &str| {
        let skill_md = world.path(root).join(to).join("SKILL.md");
        fs::read_to_string(skill_md)
            .is_ok_and(|t| frontmatter_fields(&t).get("name").map(String::as_str) == Some(to))
    };
    if named(PARKED) {
        LinkState::Off
    } else if named(SHARED) || named(CLAUDE) {
        LinkState::Elsewhere
    } else {
        LinkState::Missing
    }
}

/// Both directions: every link the oracle expects from a skill an agent
/// finds is reported for that agent with the oracle's state, and every
/// strong link reported for an agent comes from a skill that agent finds.
fn check(world: &World) -> Result<(), TestCaseError> {
    let report = skill_links(
        &runtime(world.home.path()),
        &ctx(),
        &SkillLinksRequest::default(),
    )
    .unwrap();
    for agent in AGENTS {
        let found = listing(world, agent);
        for (name, dir) in &found {
            for (to, path) in expected_links(name, dir) {
                let expected = oracle_state(world, &found, dir, &to, path.as_deref());
                let reported = report
                    .links
                    .iter()
                    .filter(|l| l.from == *name && l.to == to && l.path == path)
                    .filter(|l| matches!(l.strength, LinkStrength::Needs | LinkStrength::HandsOff))
                    .find(|l| l.agents.iter().any(|a| a.agent.as_str() == agent))
                    .and_then(|l| l.agents.iter().find(|a| a.agent.as_str() == agent))
                    .map(|a| a.state);
                prop_assert_eq!(reported, Some(expected), "{} -> {} for {}", name, to, agent);
            }
        }
        for link in report
            .links
            .iter()
            .filter(|l| matches!(l.strength, LinkStrength::Needs | LinkStrength::HandsOff))
        {
            if link.agents.iter().any(|a| a.agent.as_str() == agent) {
                prop_assert!(
                    found.iter().any(|(n, _)| *n == link.from),
                    "{} does not find {}, but its link to {} is reported",
                    agent,
                    link.from,
                    link.to
                );
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]
    #[test]
    fn links_match_what_each_agent_can_reach_after_any_sequence_of_actions(
        layout in prop_oneof![Just(Layout::Links), Just(Layout::Whole), Just(Layout::Copies)],
        steps in prop::collection::vec(step(), 1..8),
    ) {
        let catalog = real_skills();
        let world = World::new(layout);
        for step in &steps {
            apply(&world, &catalog, step);
            check(&world)?;
        }
    }
}
