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
const LOAD_WORDS: [&str; 5] = ["load", "loads", "read", "reads", "first"];

struct Found {
    to: String,
    strength: LinkStrength,
    path: Option<String>,
    file: String,
    line: u32,
    text: String,
    list_line: bool,
}

/// Finds every link between the skills in `skills`, and to `other_names`
/// (installed skills or pack listings outside the set), following the rules
/// of `docs/spec-skill-links.md` section 2.
pub fn detect_links(skills: &[SkillText], other_names: &BTreeSet<String>) -> DetectedLinks {
    let mut names: BTreeSet<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    names.extend(other_names.iter().map(String::as_str));
    let by_folder: BTreeMap<&str, &str> = skills
        .iter()
        .map(|s| (s.folder.as_str(), s.name.as_str()))
        .collect();

    let mut found: BTreeMap<&str, Vec<Found>> = BTreeMap::new();
    for skill in skills {
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
                scan_line(skill, at, line, &names, &by_folder, &mut rows);
            }
        }
        found.insert(skill.name.as_str(), rows);
    }

    let indexes: BTreeSet<&str> = skills
        .iter()
        .filter(|skill| is_index(skill, skills, &found[skill.name.as_str()]))
        .map(|skill| skill.name.as_str())
        .collect();

    // In an index, a list item or table row is an entry in the menu, whatever
    // words it uses; its prose lines keep their own strength.
    let mut best: BTreeMap<(&str, &str), (LinkStrength, &Found)> = BTreeMap::new();
    for (from, rows) in &found {
        for row in rows {
            let strength = if indexes.contains(from) && row.list_line {
                LinkStrength::Lists
            } else {
                row.strength
            };
            let entry = best
                .entry((from, row.to.as_str()))
                .or_insert((strength, row));
            if strength < entry.0 {
                *entry = (strength, row);
            }
        }
    }
    let links = best
        .into_iter()
        .map(|((from, _), (strength, row))| SkillLink {
            from: from.to_string(),
            to: row.to.clone(),
            strength,
            path: row.path.clone(),
            evidence: LinkEvidence {
                file: row.file.clone(),
                line: row.line,
                text: row.text.clone(),
            },
        })
        .collect();
    DetectedLinks {
        links,
        indexes: indexes.into_iter().map(str::to_string).collect(),
    }
}

/// An index names at least 5 skills and at least half of its pack, and at
/// least half of the lines that name them are list items or table rows.
fn is_index(skill: &SkillText, skills: &[SkillText], rows: &[Found]) -> bool {
    let targets: BTreeSet<&str> = rows.iter().map(|row| row.to.as_str()).collect();
    let pack_size = skills
        .iter()
        .filter(|other| other.pack == skill.pack && other.name != skill.name)
        .count();
    let lines: BTreeSet<(&str, u32)> = rows
        .iter()
        .map(|row| (row.file.as_str(), row.line))
        .collect();
    let list_lines: BTreeSet<(&str, u32)> = rows
        .iter()
        .filter(|row| row.list_line)
        .map(|row| (row.file.as_str(), row.line))
        .collect();
    targets.len() >= INDEX_MIN_TARGETS
        && targets.len() * 2 >= pack_size
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
    let lower = line.to_lowercase();
    let optional = OPTIONAL.iter().any(|word| lower.contains(word));
    let row = |to: &str, strength: LinkStrength, path: Option<String>| Found {
        to: to.to_string(),
        strength,
        path,
        file: file.to_string(),
        line: u32::try_from(index + 1).unwrap_or(u32::MAX),
        text: cut(line, EVIDENCE_CHARS),
        list_line,
    };
    let depth = file.matches('/').count();
    for token in path_tokens(line) {
        let ups = token.matches("../").count();
        let rest = &token[ups * 3..];
        let Some(folder) = rest.split('/').next().filter(|first| !first.is_empty()) else {
            continue;
        };
        // A path that stays inside this folder is internal, including a
        // bundled copy of a sibling (`remotion-best-practices/remotion-studio/`).
        let known = by_folder.contains_key(folder) || names.contains(folder);
        if ups != depth + 1
            || folder == skill.folder
            || !(known
                || std::path::Path::new(rest)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md")))
        {
            continue;
        }
        let to = by_folder.get(folder).copied().unwrap_or(folder);
        if to != skill.name && !negated(line, line.find(token).unwrap_or(0)) {
            let strength = if optional {
                LinkStrength::Mentions
            } else {
                LinkStrength::Needs
            };
            rows.push(row(to, strength, Some(token.to_string())));
        }
    }
    let first_dir = file.split('/').next().unwrap_or("");
    let cleaned = strip_paths(line);
    // A Skill tool call names its target outright, so it counts even when
    // that skill is not installed: the case the user most needs to see.
    if lower.contains("skill tool") {
        for quoted in cleaned.split('"').skip(1).step_by(2) {
            if !names.contains(quoted) && quoted != skill.name && is_skill_name(quoted) {
                let strength = if optional {
                    LinkStrength::Mentions
                } else {
                    LinkStrength::HandsOff
                };
                rows.push(row(quoted, strength, None));
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
            if let Some(strength) = classify(&cleaned, &cleaned.to_lowercase(), start, end) {
                let strength = if optional {
                    strength.max(LinkStrength::Mentions)
                } else {
                    strength
                };
                rows.push(row(name, strength, None));
                break;
            }
        }
    }
}

/// The strength of one occurrence of a name, or `None` when it is not a link.
fn classify(line: &str, lower: &str, start: usize, end: usize) -> Option<LinkStrength> {
    let before = &line[..start];
    let after = &line[end..];
    let before_lower = tail(&lower[..start], 60);
    if negated(line, start) || before_lower.contains("called by") {
        return None;
    }
    let ticked = before.ends_with('`') && after.starts_with('`');
    let bold = before.ends_with("**") && after.starts_with("**");
    let quoted = before.ends_with('"') && after.starts_with('"');
    let after_mark = after.trim_start_matches(['`', '*', '"']).to_lowercase();
    if before.ends_with("npx ")
        || after_mark.starts_with(" cli")
        || after_mark.starts_with(" command")
    {
        return None;
    }
    let slash = before.ends_with('/') && !before.ends_with("./") && {
        let prior = before[..before.len() - 1].chars().next_back();
        prior.is_none_or(|c| !c.is_alphanumeric() && c != '.' && c != '_')
    };
    let namespaced = before
        .strip_suffix(':')
        .and_then(|rest| rest.chars().next_back())
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-');
    let skill_tool = lower.contains("skill tool");
    let names_skill = after_mark.starts_with(" skill") || after_mark.starts_with(" sub-skill");
    let verb = ["invoke", "run the", "use the"]
        .iter()
        .any(|v| before_lower.contains(v));
    if slash || namespaced || (skill_tool && (quoted || ticked)) || (verb && names_skill) {
        return Some(LinkStrength::HandsOff);
    }
    let marked = ticked || bold || before.ends_with('[') && after.starts_with(']');
    let words: BTreeSet<&str> = before_lower.split(|c: char| !c.is_alphanumeric()).collect();
    if marked && LOAD_WORDS.iter().any(|word| words.contains(word)) {
        return Some(LinkStrength::Needs);
    }
    if marked || names_skill {
        return Some(LinkStrength::Mentions);
    }
    None
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

fn negated(line: &str, start: usize) -> bool {
    let lower = line[..start].to_lowercase();
    let before = tail(&lower, 30);
    NEGATIONS.iter().any(|word| before.contains(word))
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
        out.push(&line[start..end]);
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

fn tail(s: &str, chars: usize) -> &str {
    let skip = s.chars().count().saturating_sub(chars);
    s.char_indices().nth(skip).map_or(s, |(at, _)| &s[at..])
}

fn cut(s: &str, chars: usize) -> String {
    s.chars().take(chars).collect()
}
