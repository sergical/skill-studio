//! Shared invocation policy text edits.
use crate::frontmatter::{invocation_policy, parse_frontmatter, InvocationPolicy};
use std::fmt::Write as _;
fn strip_terminator(raw: &str) -> &str {
    raw.strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .unwrap_or(raw)
}

/// A line at column 0 (no leading whitespace) with some content - the start
/// of a new top-level YAML key. Blank lines and indented lines are
/// continuations of whatever top-level key preceded them (a nested mapping,
/// a block scalar body, or just blank padding).
fn is_top_level_line(text: &str) -> bool {
    !text.is_empty() && !text.starts_with(' ') && !text.starts_with('\t')
}

/// Whether `text` (a top-level line) is the given top-level `key`, i.e.
/// matches `^<key>\s*:`.
fn is_key(text: &str, key: &str) -> bool {
    match text.strip_prefix(key) {
        Some(rest) => rest.trim_start_matches([' ', '\t']).starts_with(':'),
        None => false,
    }
}

/// Groups `body` (the frontmatter's lines, one entry per line, sans
/// terminator) into `[start, end)` spans, one per top-level key: a span
/// starts at a column-0 line and extends through every blank or indented
/// line that follows, up to (but not including) the next column-0 line.
fn top_level_spans(body: &[&str]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < body.len() {
        if !is_top_level_line(body[i]) {
            // Malformed frontmatter (content before any top-level key) -
            // skip rather than looping forever; nothing to attach it to.
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < body.len()
            && (body[i].is_empty() || body[i].starts_with(' ') || body[i].starts_with('\t'))
        {
            i += 1;
        }
        spans.push((start, i));
    }
    spans
}

/// Removes (or replaces) the top-level `disable-model-invocation`/
/// `user-invocable` keys in `content`'s frontmatter block to match `policy`,
/// inserting the new key (if any) right after the `description` key's span -
/// after its block-scalar body, if it has one - or at the end of the
/// frontmatter when there's no `description`. Every other byte - other keys
/// (including a nested key that happens to share a name with one of these
/// two), the body, blank lines, the line separator style (`\r\n` vs `\n`),
/// and a missing final newline - is passed through unchanged. Errs when
/// `content` has no `---`-fenced frontmatter block to edit, or when the
/// result doesn't parse back to the requested `policy`.
pub fn rewrite_invocation_frontmatter(
    content: &str,
    policy: InvocationPolicy,
) -> Result<String, String> {
    let sep = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    // Raw segments keep each line's own terminator (or lack of one, for the
    // last line) attached, so untouched lines can be re-emitted byte for
    // byte instead of being rejoined with a terminator we chose ourselves.
    let raw_lines: Vec<&str> = content.split_inclusive('\n').collect();
    let lines: Vec<&str> = raw_lines.iter().copied().map(strip_terminator).collect();

    if lines.first().map(|l| l.trim()) != Some("---") {
        return Err("SKILL.md has no frontmatter to edit".to_string());
    }
    let close_idx = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i)
        .ok_or("SKILL.md frontmatter has no closing `---`")?;

    let body: &[&str] = &lines[1..close_idx];
    let body_raw: &[&str] = &raw_lines[1..close_idx];
    let spans = top_level_spans(body);

    let mut drop = vec![false; body.len()];
    for &(start, end) in &spans {
        if is_key(body[start], "disable-model-invocation") || is_key(body[start], "user-invocable")
        {
            for slot in drop.iter_mut().take(end).skip(start) {
                *slot = true;
            }
        }
    }
    let description_span = spans
        .iter()
        .find(|&&(start, _)| is_key(body[start], "description"))
        .copied();

    let new_key = match policy {
        InvocationPolicy::Both => None,
        InvocationPolicy::UserOnly => Some("disable-model-invocation: true"),
        InvocationPolicy::ModelOnly => Some("user-invocable: false"),
    };

    let mut out = String::new();
    out.push_str(raw_lines[0]);
    for idx in 0..body.len() {
        if drop[idx] {
            continue;
        }
        out.push_str(body_raw[idx]);
        let at_description_end = description_span.is_some_and(|(_, end)| idx == end - 1);
        if at_description_end {
            if let Some(key) = new_key {
                out.push_str(key);
                out.push_str(sep);
            }
        }
    }
    if description_span.is_none() {
        if let Some(key) = new_key {
            out.push_str(key);
            out.push_str(sep);
        }
    }
    out.push_str(raw_lines[close_idx]);
    for raw in &raw_lines[close_idx + 1..] {
        out.push_str(raw);
    }

    let parsed = parse_frontmatter(&out);
    let rewritten = parsed
        .as_frontmatter()
        .ok_or("Rewritten frontmatter failed to parse back".to_string())?;
    let (rewritten_policy, _) = invocation_policy(Some(rewritten));
    if rewritten_policy != policy {
        return Err(
            "Rewritten frontmatter does not round-trip to the requested invocation policy"
                .to_string(),
        );
    }

    Ok(out)
}

/// Update only the Codex policy value. Other YAML bytes stay unchanged.
pub fn rewrite_codex_yaml(content: &str, manual: bool) -> Result<String, String> {
    let value: serde_yaml::Value = if content.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(content).map_err(|e| e.to_string())?
    };
    let mapping = value.as_mapping().ok_or("openai.yaml is not a mapping")?;
    let sep = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let flag = if manual { "false" } else { "true" };
    let lines: Vec<_> = content.split_inclusive('\n').collect();
    if let Some(policy) = mapping.get("policy") {
        policy
            .as_mapping()
            .ok_or("openai.yaml policy is not a mapping")?;
        let start = lines
            .iter()
            .position(|line| is_key(strip_terminator(line), "policy"));
        let block = start.is_some_and(|start| {
            strip_terminator(lines[start])
                .split_once(':')
                .is_some_and(|(_, rest)| rest.trim().is_empty() || rest.trim().starts_with('#'))
        });
        if !block {
            // Flow mappings and quoted keys have no line span to patch safely.
            let mut root = mapping.clone();
            let mut policy = policy
                .as_mapping()
                .ok_or("openai.yaml policy is not a mapping")?
                .clone();
            policy.insert("allow_implicit_invocation".into(), (!manual).into());
            root.insert("policy".into(), policy.into());
            return serde_yaml::to_string(&root).map_err(|e| e.to_string());
        }
        let start = start.ok_or("policy has no editable span")?;
        let end = (start + 1..lines.len())
            .find(|&i| is_top_level_line(strip_terminator(lines[i])) && !lines[i].starts_with('#'))
            .unwrap_or(lines.len());
        let mut out = String::new();
        let mut found = false;
        let indent = lines[start + 1..end]
            .iter()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map_or(2, |line| line.len() - line.trim_start().len());
        for (i, line) in lines.iter().enumerate() {
            if i > start
                && i < end
                && line.len() - line.trim_start().len() == indent
                && is_key(line.trim_start(), "allow_implicit_invocation")
            {
                let comment = strip_terminator(line)
                    .split_once('#')
                    .map(|(_, c)| format!(" #{c}"))
                    .unwrap_or_default();
                let _ = write!(
                    out,
                    "{}allow_implicit_invocation: {flag}{comment}{sep}",
                    " ".repeat(indent)
                );
                found = true;
            } else {
                out.push_str(line);
            }
            if i + 1 == end && !found {
                if !out.ends_with('\n') {
                    out.push_str(sep);
                }
                let _ = write!(
                    out,
                    "{}allow_implicit_invocation: {flag}{sep}",
                    " ".repeat(indent)
                );
            }
        }
        Ok(out)
    } else {
        let mut out = content.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(sep);
        }
        let _ = write!(out, "policy:{sep}  allow_implicit_invocation: {flag}{sep}");
        Ok(out)
    }
}
