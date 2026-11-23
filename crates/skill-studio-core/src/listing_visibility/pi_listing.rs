//! pi 1.6.0 `DefaultResourceLoader` precedence and `formatSkillsForPrompt`, for scanned roots.
use super::{
    folder_listing, HarnessListing, ListingDescription, ListingEntry, ListingEvidence, ListingState,
};
use crate::dto::Inventory;
use crate::harness::HarnessFacts;
use crate::identity::{RootKind, RootScope};
use crate::ports::Runtime;
use std::collections::BTreeSet;

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn pi_base_prompt(
    rt: &Runtime,
    inventory: &Inventory,
    facts: &HarnessFacts,
    scope: &RootScope,
) -> HarnessListing {
    let mut candidates = Vec::new();
    for skill in &inventory.skills {
        for deployment in &skill.deployments {
            if deployment.root.scope != RootScope::Global && &deployment.root.scope != scope {
                continue;
            }
            let shared = deployment.root.kind == RootKind::Universal;
            if !(shared || deployment.harness.as_ref() == Some(&facts.id))
                || deployment.symlink_is_broken
                || deployment.studio_disabled
                || deployment.disabled_readers.contains(&facts.id)
                || deployment.disabled_by == Some(crate::harness::DisabledBy::PiSettings)
                || deployment.root.kind == RootKind::Parked
            {
                continue;
            }
            if let Some(frontmatter) = &deployment.frontmatter {
                if let Some(description) = frontmatter
                    .description
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                {
                    let name = frontmatter
                        .name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&skill.name.0);
                    candidates.push((
                        (
                            deployment.root.scope == RootScope::Global,
                            shared,
                            &deployment.path,
                        ),
                        name,
                        description,
                        frontmatter.disable_model_invocation == Some(true),
                    ));
                }
            }
        }
    }
    // Project pi, project Universal, user pi, user Universal. The first valid name and file win,
    // even when that skill is manual-only. The formatter preserves the winning lexical location.
    candidates.sort_by_key(|candidate| candidate.0);
    let mut files = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut entries = Vec::new();
    let mut xml = String::new();
    for ((_, _, path), name, description, manual) in candidates {
        let file = path.join("SKILL.md");
        let canonical = rt
            .ports
            .fs
            .canonicalize(&file)
            .unwrap_or_else(|_| file.clone());
        if files.contains(&canonical) || !names.insert(name) {
            continue;
        }
        files.insert(canonical);
        if !manual {
            use std::fmt::Write;
            let _ = write!(xml, "\n  <skill>\n    <name>{}</name>\n    <description>{}</description>\n    <location>{}</location>\n  </skill>",escape_xml(name),escape_xml(description),escape_xml(&file.to_string_lossy()));
        }
        let chars = description.chars().count() as u64;
        entries.push(ListingEntry {
            name: name.to_owned(),
            path: Some(path.clone()),
            plugin: None,
            state: if manual {
                ListingState::Hidden
            } else {
                ListingState::Described
            },
            hidden_reason: manual.then(|| "manual-only".into()),
            description_preview: (!manual).then(|| ListingDescription {
                shown: description.into(),
                remainder: String::new(),
            }),
            shown_description_chars: if manual { 0 } else { chars },
            full_description_chars: chars,
            usage_score: None,
            observed: None,
            scanned: true,
            observed_description_chars: None,
            invocation_mismatch: None,
        });
    }
    if !xml.is_empty() {
        xml = format!("<skills>\nThe following skills provide specialized instructions for specific tasks.\nUse the read tool to load a skill's file when the task matches its description.\nResolve relative paths against the skill's directory (parent of SKILL.md). Use absolute paths in tool calls.\n\n<available_skills>{xml}\n</available_skills>\n</skills>");
    }
    let mut row = folder_listing(facts, scope, entries, xml.chars().count() as u64);
    row.evidence = ListingEvidence::Preview;
    row.budget_source = "pi 1.6.0 base prompt".into();
    row.base_prompt_skills = Some(xml);
    row.notes.push("Scanned default skill folders with the read tool enabled and the selected project trusted. Custom resources, tool selection and extensions can change a session’s prompt.".into());
    row
}
