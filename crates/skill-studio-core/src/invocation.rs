//! Invocation edits preserve all bytes outside the requested scalar values.
use crate::frontmatter::{invocation_policy, parse_frontmatter, InvocationPolicy};

fn key_value(line: &str, key: &str) -> Option<usize> {
    let (name, _) = line.split_once(':')?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(name.trim()).ok()?;
    (parsed.as_str() == Some(key)).then_some(name.len() + 1)
}

fn parse_yaml(content: &str) -> Result<serde_yaml::Value, String> {
    if content.trim().is_empty() {
        Ok(serde_yaml::Value::Mapping(Default::default()))
    } else {
        serde_yaml::from_str(content).map_err(|e| e.to_string())
    }
}

fn set_value(root: &mut serde_yaml::Value, keys: &[&str], flag: bool) -> Result<(), String> {
    let map = root
        .as_mapping_mut()
        .ok_or("invocation YAML is not a mapping")?;
    if keys.len() == 1 {
        map.insert(keys[0].into(), flag.into());
    } else {
        let child = map
            .entry(keys[0].into())
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
        set_value(child, &keys[1..], flag)?;
    }
    Ok(())
}

fn patch_block(content: &str, keys: &[&str], flag: bool) -> Result<String, String> {
    let sep = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines: Vec<_> = content.split_inclusive('\n').collect();
    let mut start = 0;
    let mut end = lines.len();
    let mut indent = 0;
    let mut offset = 0;
    for (depth, key) in keys.iter().enumerate() {
        let found = (start..end).find(|&i| {
            let line = lines[i];
            line.len() - line.trim_start_matches(' ').len() == indent
                && key_value(line.trim_start_matches(' '), key).is_some()
        });
        let Some(i) = found else {
            let insertion = if keys.len() == 1 {
                lines
                    .iter()
                    .position(|line| key_value(line, "description").is_some())
                    .map_or(end, |description| {
                        (description + 1..end)
                            .find(|&j| {
                                !lines[j].trim().is_empty()
                                    && !lines[j].starts_with([' ', '\t', '#'])
                            })
                            .unwrap_or(end)
                    })
            } else {
                end
            };
            let at: usize = lines[..insertion].iter().map(|l| l.len()).sum();
            let mut addition = String::new();
            if at > 0 && !content[..at].ends_with('\n') {
                addition.push_str(sep);
            }
            for (j, key) in keys[depth..].iter().enumerate() {
                addition.push_str(&" ".repeat(indent + j * 2));
                addition.push_str(key);
                addition.push(':');
                if depth + j + 1 == keys.len() {
                    addition.push_str(if flag { " true" } else { " false" });
                }
                addition.push_str(sep);
            }
            return Ok(format!("{}{}{}", &content[..at], addition, &content[at..]));
        };
        offset = lines[..i].iter().map(|l| l.len()).sum::<usize>() + indent;
        let line = lines[i].trim_start_matches(' ');
        let colon = key_value(line, key).ok_or("invocation key has no editable span")?;
        let rest = &line[colon..];
        if depth + 1 == keys.len() {
            let leading = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            let value = rest.trim_start_matches([' ', '\t']);
            let len = value
                .find(|c: char| c.is_whitespace() || c == '#')
                .unwrap_or(value.len());
            if !matches!(&value[..len], "true" | "false" | "0" | "1") {
                return Err(
                    "unsupported invocation YAML layout; use a block mapping with boolean values"
                        .into(),
                );
            }
            let at = offset + colon + leading;
            return Ok(format!(
                "{}{}{}",
                &content[..at],
                flag,
                &content[at + len..]
            ));
        }
        if !rest.trim().is_empty() && !rest.trim_start().starts_with('#') {
            return Err("unsupported invocation YAML layout; use a block mapping".into());
        }
        start = i + 1;
        end = (start..end)
            .find(|&j| {
                let line = lines[j];
                !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                    && line.len() - line.trim_start_matches(' ').len() <= indent
            })
            .unwrap_or(end);
        indent = lines[start..end]
            .iter()
            .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .map_or(indent + 2, |l| l.len() - l.trim_start_matches(' ').len());
    }
    Err(format!("invocation key has no editable span at {offset}"))
}

fn patch_flag(content: &str, keys: &[&str], flag: bool) -> Result<String, String> {
    let mut intended = parse_yaml(content)?;
    set_value(&mut intended, keys, flag)?;
    let out = patch_block(content, keys, flag)?;
    if parse_yaml(&out)? != intended {
        return Err(
            "unsupported invocation YAML layout; proposed edit does not match the requested policy"
                .into(),
        );
    }
    Ok(out)
}

/// Change invocation scalar values without removing comments or line endings.
pub fn rewrite_invocation_frontmatter(
    content: &str,
    policy: InvocationPolicy,
) -> Result<String, String> {
    let lines: Vec<_> = content.split_inclusive('\n').collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return Err("SKILL.md has no frontmatter to edit".into());
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i)
        .ok_or("SKILL.md frontmatter has no closing `---`")?;
    let mut body = lines[1..close].concat();
    let original = parse_yaml(&body)?;
    for (key, flag) in [
        (
            "disable-model-invocation",
            policy == InvocationPolicy::UserOnly,
        ),
        ("user-invocable", policy != InvocationPolicy::ModelOnly),
    ] {
        if original.get(key).is_some()
            || (key == "disable-model-invocation" && flag)
            || (key == "user-invocable" && !flag)
        {
            body = patch_flag(&body, &[key], flag)?;
        }
    }
    if original
        .get("metadata")
        .and_then(|m| m.get("opencode/autoinvoke"))
        .is_some_and(|value| {
            value.as_bool().or_else(|| value.as_i64().map(|n| n == 1))
                != Some(policy != InvocationPolicy::UserOnly)
        })
    {
        body = patch_flag(
            &body,
            &["metadata", "opencode/autoinvoke"],
            policy != InvocationPolicy::UserOnly,
        )?;
    }
    let out = format!("{}{}{}", lines[0], body, lines[close..].concat());
    let parsed = parse_frontmatter(&out);
    let rewritten = parsed
        .as_frontmatter()
        .ok_or("Rewritten frontmatter failed to parse back")?;
    if invocation_policy(Some(rewritten)).0 != policy {
        return Err(
            "Rewritten frontmatter does not round-trip to the requested invocation policy".into(),
        );
    }
    Ok(out)
}

/// Update only the Codex policy value, refusing layouts without a safe span.
pub fn rewrite_codex_yaml(content: &str, manual: bool) -> Result<String, String> {
    patch_flag(content, &["policy", "allow_implicit_invocation"], !manual)
}
