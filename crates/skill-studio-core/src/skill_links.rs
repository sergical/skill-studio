//! Skill links: lines in one skill's folder that name another skill.
//!
//! `docs/spec-skill-links.md` sections 2.1 and 2.2 define what counts. This
//! module holds no filesystem access; a host adapter hands it the Markdown
//! text of each skill folder.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How much a skill depends on the skill it names. Declared strongest first,
/// so the smaller value wins when one skill names another more than once.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LinkStrength {
    /// Reads the other skill's files, or loads it before it starts.
    Needs,
    /// Runs the other skill for a step, or sends the user to it.
    HandsOff,
    /// Names it as related, optional or see-also.
    Mentions,
    /// An index skill lists it as one of its entries.
    Lists,
}

/// The line that made a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LinkEvidence {
    /// File inside the linking skill's folder, with `/` separators.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// The line, trimmed and cut to at most 200 characters.
    pub text: String,
}

/// One skill naming another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SkillLink {
    /// The skill whose text names the other.
    pub from: String,
    /// The skill it names.
    pub to: String,
    /// The strongest way any line in `from` names `to`.
    pub strength: LinkStrength,
    /// For a path link, the path as written, relative to `evidence.file`.
    pub path: Option<String>,
    /// The first line with that strength.
    pub evidence: LinkEvidence,
}

/// The Markdown files of one skill folder.
#[derive(Debug, Clone)]
pub struct SkillText {
    /// The skill's name.
    pub name: String,
    /// The skill folder's own name, which a `../<folder>/` path uses.
    pub folder: String,
    /// The pack it came from, when known. The index rule compares a skill
    /// with the rest of its pack.
    pub pack: Option<String>,
    /// `(path inside the folder with / separators, text)` for each `.md` file.
    pub files: Vec<(String, String)>,
}

/// Links found in a set of skills, and which of them are indexes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DetectedLinks {
    /// One entry per `(from, to)`, sorted.
    pub links: Vec<SkillLink>,
    /// Marked names in the text that no installation record identifies.
    pub unverified: Vec<SkillLink>,
    /// Skills that list most of their pack, sorted.
    pub indexes: Vec<String>,
}

const EVIDENCE_CHARS: usize = 200;
const INDEX_MIN_TARGETS: usize = 5;
const NEGATIONS: [&str; 8] = [
    "do not call",
    "don't call",
    "never call",
    "do not use",
    "don't use",
    "never use",
    "do not invoke",
    "不要",
];
const OPTIONAL: [&str; 4] = ["if available", "when available", "if installed", "optional"];

struct Found {
    to: String,
    strength: LinkStrength,
    path: Option<String>,
    file: String,
    line: u32,
    text: String,
    list_line: bool,
    unverified: bool,
}

/// Finds every link between the skills in `skills`, and to `other_names`
/// (installed skills or pack listings outside the set), following the rules
/// of `docs/spec-skill-links.md` section 2.
pub fn detect_links(skills: &[SkillText], other_names: &BTreeSet<String>) -> DetectedLinks {
    let vocabulary = LinkVocabulary::new(skills.iter(), other_names);
    let mut result = DetectedLinks::default();
    for skill in skills {
        let mut one = vocabulary.detect(skill);
        result.links.append(&mut one.links);
        result.unverified.append(&mut one.unverified);
        result.indexes.append(&mut one.indexes);
    }
    result
        .links
        .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    result
        .unverified
        .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    result.indexes.sort();
    result
}

pub(crate) struct LinkVocabulary<'a> {
    names: BTreeSet<&'a str>,
    by_folder: BTreeMap<&'a str, &'a str>,
    packs: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

impl<'a> LinkVocabulary<'a> {
    pub fn new(
        skills: impl Iterator<Item = &'a SkillText>,
        other_names: &'a BTreeSet<String>,
    ) -> Self {
        let mut names: BTreeSet<_> = other_names.iter().map(String::as_str).collect();
        let mut by_folder = BTreeMap::new();
        let mut packs = BTreeMap::<_, BTreeSet<_>>::new();
        for skill in skills {
            names.insert(skill.name.as_str());
            by_folder.insert(skill.folder.as_str(), skill.name.as_str());
            if let Some(pack) = skill.pack.as_deref() {
                packs.entry(pack).or_default().insert(skill.name.as_str());
            }
        }
        Self {
            names,
            by_folder,
            packs,
        }
    }

    pub fn detect(&self, skill: &SkillText) -> DetectedLinks {
        let mut rows = Vec::new();
        for (file, text) in &skill.files {
            let mut in_list = false;
            let mut in_fence = false;
            for (index, raw) in text.lines().enumerate() {
                let line = raw.trim();
                // Code samples hold import paths and commands, not instructions.
                if line.starts_with("```") || line.starts_with("~~~") {
                    in_fence = !in_fence;
                    continue;
                }
                if in_fence {
                    continue;
                }
                // An indented line under a list item belongs to that item.
                in_list = is_list_line(line)
                    || (in_list && (line.is_empty() || raw.starts_with([' ', '\t'])));
                let at = LineAt {
                    file,
                    index,
                    list_line: in_list,
                };
                scan_line(skill, at, line, &self.names, &self.by_folder, &mut rows);
            }
        }
        let index = skill
            .pack
            .as_deref()
            .and_then(|p| self.packs.get(p))
            .is_some_and(|members| is_index(members, &rows));
        let mut best = BTreeMap::new();
        for row in &rows {
            let strength = if !row.unverified
                && index
                && row.list_line
                && row.strength == LinkStrength::Mentions
            {
                LinkStrength::Lists
            } else {
                row.strength
            };
            let entry = best.entry(row.to.as_str()).or_insert((strength, row));
            if (row.unverified, strength) < (entry.1.unverified, entry.0) {
                *entry = (strength, row);
            }
        }
        let (mut links, mut unverified) = (Vec::new(), Vec::new());
        for (strength, row) in best.into_values() {
            let dest = if row.unverified {
                &mut unverified
            } else {
                &mut links
            };
            dest.push(SkillLink {
                from: skill.name.clone(),
                to: row.to.clone(),
                strength,
                path: row.path.clone(),
                evidence: LinkEvidence {
                    file: row.file.clone(),
                    line: row.line,
                    text: row.text.clone(),
                },
            });
        }
        DetectedLinks {
            links,
            unverified,
            indexes: index.then(|| skill.name.clone()).into_iter().collect(),
        }
    }
}
/// An index names at least 5 skills and at least half of its pack, and at
/// least half of the lines that name them are list items or table rows.
fn is_index(members: &BTreeSet<&str>, rows: &[Found]) -> bool {
    let rows: Vec<_> = rows.iter().filter(|r| !r.unverified).collect();
    let targets: BTreeSet<&str> = rows.iter().map(|row| row.to.as_str()).collect();
    let lines: BTreeSet<(&str, u32)> = rows
        .iter()
        .map(|row| (row.file.as_str(), row.line))
        .collect();
    let list_lines: BTreeSet<(&str, u32)> = rows
        .iter()
        .filter(|row| row.list_line)
        .map(|row| (row.file.as_str(), row.line))
        .collect();
    // A skill with no known pack has nothing to be an index of: counting all
    // such skills together would make the answer depend on unrelated installs.
    targets.intersection(members).count() >= INDEX_MIN_TARGETS
        && targets.intersection(members).count() * 2 >= members.len().saturating_sub(1)
        && list_lines.len() * 2 >= lines.len()
}

#[derive(Clone, Copy)]
struct LineAt<'a> {
    file: &'a str,
    index: usize,
    list_line: bool,
}

fn scan_line(
    skill: &SkillText,
    at: LineAt<'_>,
    line: &str,
    names: &BTreeSet<&str>,
    by_folder: &BTreeMap<&str, &str>,
    rows: &mut Vec<Found>,
) {
    if line.is_empty() {
        return;
    }
    let LineAt {
        file,
        index,
        list_line,
    } = at;

    let row = |to: &str, strength: LinkStrength, path: Option<String>| Found {
        to: to.to_string(),
        strength,
        path,
        file: file.to_string(),
        line: u32::try_from(index + 1).unwrap_or(u32::MAX),
        text: cut(line, EVIDENCE_CHARS),
        list_line,
        unverified: false,
    };
    let depth = file.matches('/').count();
    for token in path_tokens(line) {
        let ups = token.split('/').take_while(|part| *part == "..").count();
        let rest = &token[ups * 3..];
        let Some(folder) = rest.split('/').next().filter(|first| !first.is_empty()) else {
            continue;
        };
        // A path that stays inside this folder is internal, including a
        // bundled copy of a sibling (`remotion-best-practices/remotion-studio/`).
        let known = by_folder.contains_key(folder) || names.contains(folder);
        if rest.split('/').any(|part| matches!(part, "" | "." | ".."))
            || ups != depth + 1
            || folder == skill.folder
            || !(known || rest.ends_with("/SKILL.md"))
        {
            continue;
        }
        let to = by_folder.get(folder).copied().unwrap_or(folder);
        if to != skill.name && !negated(line, line.find(token).unwrap_or(0)) {
            let strength = if optional_use(line, line.find(token).unwrap_or(0)) {
                LinkStrength::Mentions
            } else {
                LinkStrength::Needs
            };
            rows.push(row(to, strength, Some(token.to_string())));
        }
    }
    let first_dir = file.split('/').next().unwrap_or("");
    let cleaned = strip_paths(line);
    let lower = cleaned.to_ascii_lowercase();
    for (pos, _) in lower.match_indices("the skill tool ") {
        let before = lower[..pos].trim_end();
        if !matches!(
            before.split_whitespace().next_back(),
            Some("call" | "calls" | "calling")
        ) || negated(&cleaned, pos)
        {
            continue;
        }
        let args = &cleaned[pos + "the skill tool ".len()..];
        let Some(mut args) = args
            .strip_prefix("with ")
            .or_else(|| args.strip_prefix("twice, for "))
        else {
            continue;
        };
        while let Some(quote) = args
            .chars()
            .next()
            .filter(|q| matches!(q, '"' | '`' | '\''))
        {
            let Some((name, rest)) = args[1..].split_once(quote) else {
                break;
            };
            if name != skill.name && is_skill_name(name) {
                let strength = if optional_use(&cleaned, pos) || rest.starts_with(" where possible")
                {
                    LinkStrength::Mentions
                } else {
                    LinkStrength::HandsOff
                };
                rows.push(row(name, strength, None));
            }
            let Some(more) = rest
                .strip_prefix(" and ")
                .or_else(|| rest.strip_prefix(", "))
            else {
                break;
            };
            args = more;
        }
    }
    // A cross-plugin instruction supplies its own source even on a machine
    // where the other plugin has never been installed.
    for (pos, _) in cleaned.match_indices("`/") {
        let Some((name, after)) = cleaned[pos + 2..].split_once('`') else {
            continue;
        };
        if let Some(source) = after
            .strip_prefix(" from the `")
            .and_then(|s| s.split_once("` plugin"))
        {
            if is_skill_name(source.0)
                && is_skill_name(name)
                && name != skill.name
                && !negated(&cleaned, pos)
            {
                rows.push(row(name, LinkStrength::Mentions, None));
            }
        }
    }
    // Preserve an author's literal reference when the last record disappears.
    // The UI can show its line without claiming the token is an installed skill.
    for mark in ["`", "\"", "**"] {
        for literal in cleaned.split(mark).skip(1).step_by(2) {
            let name = match literal.strip_prefix('/') {
                Some(command) => command,
                None if mark == "**" && literal.contains('-') => literal,
                None => continue,
            };
            if !names.contains(name) && is_skill_name(name) && name != skill.name {
                rows.push(Found {
                    unverified: true,
                    ..row(name, LinkStrength::Mentions, None)
                });
            }
        }
    }
    for name in names {
        if *name == skill.name || (file.contains('/') && first_dir == *name) {
            continue;
        }
        for (start, _) in cleaned.match_indices(name) {
            let end = start + name.len();
            if !is_boundary(&cleaned, start, end) {
                continue;
            }
            if mentions(&cleaned, start, end) {
                rows.push(row(name, LinkStrength::Mentions, None));
                break;
            }
        }
    }
}

fn mentions(line: &str, start: usize, end: usize) -> bool {
    let before = &line[..start];
    let after = &line[end..];
    if negated(line, start) || clause(before).contains("called by") || before.ends_with("npx ") {
        return false;
    }
    let after_mark = after.trim_start_matches(['`', '*', '"']);
    if after_mark.starts_with(" cli") || after_mark.starts_with(" command") {
        return false;
    }
    let slash = before.strip_suffix('/').is_some_and(|prefix| {
        prefix
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || "`*[(\"'".contains(c))
    }) && !after.starts_with('/');
    slash
        || (before.ends_with('`') && after.starts_with('`'))
        || (before.ends_with("**") && after.starts_with("**"))
        || (before.ends_with('[') && after.starts_with(']'))
        || after_mark.starts_with(" skill")
        || before.ends_with(':')
            && before[..before.len() - 1].ends_with(|c: char| c.is_ascii_alphanumeric())
}

/// The Agent Skills name rule: lowercase letters, digits and single
/// hyphens, 1 to 64 characters, not starting or ending with a hyphen.
fn is_skill_name(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

fn clause(before: &str) -> &str {
    before.rsplit(['.', ';', '!', '?']).next().unwrap_or(before)
}

fn optional_use(line: &str, start: usize) -> bool {
    let before = clause(&line[..start]).to_ascii_lowercase();
    OPTIONAL.iter().any(|word| before.contains(word))
}

fn negated(line: &str, start: usize) -> bool {
    let before = clause(&line[..start]).to_ascii_lowercase();
    NEGATIONS.iter().any(|word| {
        before
            .trim_end_matches([' ', '`', '*', '/', '"'])
            .ends_with(word)
    }) || before.contains("不要")
}

fn is_boundary(line: &str, start: usize, end: usize) -> bool {
    let joins = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    !line[..start].chars().next_back().is_some_and(joins)
        && !line[end..].chars().next().is_some_and(joins)
}

fn is_list_line(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with('|')
        || line
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Relative paths that climb out of the file's folder: `../x/SKILL.md`.
fn path_tokens(line: &str) -> Vec<&str> {
    let token_char = |c: char| c.is_ascii_alphanumeric() || "_@.-/".contains(c);
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(found) = line[search..].find("../") {
        let start = search + found;
        let prior = line[..start].chars().next_back();
        if prior.is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') {
            search = start + 3;
            continue;
        }
        let end = line[start..]
            .char_indices()
            .find(|(_, c)| !token_char(*c))
            .map_or(line.len(), |(offset, _)| start + offset);
        // A path that ends a sentence keeps its period out of the file name.
        let token = line[start..end].trim_end_matches(['.', ',', ':', ';']);
        out.push(token);
        search = end.max(start + 3);
    }
    out
}

/// The line with link targets and relative paths blanked, so names inside a
/// path are judged as the path, not again as names.
fn strip_paths(line: &str) -> String {
    let mut out = line.to_string();
    for token in path_tokens(line) {
        out = out.replacen(token, &" ".repeat(token.len()), 1);
    }
    out
}

fn cut(s: &str, chars: usize) -> String {
    s.chars().take(chars).collect()
}
