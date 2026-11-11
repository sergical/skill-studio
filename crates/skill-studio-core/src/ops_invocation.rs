//! Invocation checks and journaled edits for all installed copies.
use crate::frontmatter::InvocationPolicy;
use crate::identity::{AgentId, DeploymentId, DeploymentMutability, EventId, PlanId, SkillName};
use crate::ops::{Operation, Outcome};
use crate::ports::{MutationSession, OpContext, PlanStatus, Runtime};
use crate::{CoreError, ErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
/// Check a scope or set invocation for one skill.
pub struct InvocationRequest {
    /// Name filter; required for writes.
    pub skill: Option<SkillName>,
    /// Absent checks only. A policy writes all copies of the named skill.
    pub policy: Option<InvocationPolicy>,
    /// Desktop edits one exact copy.
    pub deployment_id: Option<DeploymentId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Effective model invocation support for one harness.
pub enum InvocationMode {
    /// Only explicit invocation.
    Manual,
    /// Model invocation is allowed.
    Auto,
    /// No verified skill-owned control.
    Unsupported,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
/// One harness reading an installed copy.
pub struct InvocationDeployment {
    /// Installed copy identity.
    pub deployment_id: DeploymentId,
    /// Harness that reads this copy.
    pub harness: AgentId,
    /// Copy directory.
    pub path: PathBuf,
    /// Mode this harness reads.
    pub mode: InvocationMode,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
/// Invocation agreement across a skill's copies.
pub struct SkillInvocationReport {
    /// Installed skill name.
    pub skill: SkillName,
    /// Per-harness modes.
    pub deployments: Vec<InvocationDeployment>,
    /// Skill-owned controls or copies disagree.
    pub mismatch: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
/// Scope invocation report and undo event ids.
pub struct InvocationReport {
    /// Checked skills.
    pub skills: Vec<SkillInvocationReport>,
    /// Changes that can be undone through `restore_event`.
    pub event_ids: Vec<EventId>,
}
impl Outcome for InvocationReport {}

/// Check invocation, or set both skill-owned controls under the write lease.
pub fn invocation(
    rt: &Runtime,
    ctx: &OpContext,
    req: &InvocationRequest,
) -> Result<InvocationReport, CoreError> {
    rt.run(Operation::Invocation, ctx, || {
        let event_ids = if let Some(policy) = req.policy {
            let skill = req.skill.clone().ok_or_else(|| {
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    "choose a skill to set invocation",
                )
            })?;
            let mut session = MutationSession::begin(rt, ctx)?;
            let targets: Vec<_> = session
                .fresh
                .skills
                .iter()
                .filter(|s| s.name == skill)
                .flat_map(|s| &s.deployments)
                .filter(|d| req.deployment_id.as_ref().is_none_or(|id| id == &d.id))
                .cloned()
                .collect();
            if targets.is_empty() {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "no installed copy matches",
                ));
            }
            let fs = rt.ports.fs.as_ref();
            let mut edits = Vec::new();
            for d in &targets {
                let dir = fs
                    .canonicalize(&d.path)
                    .map_err(|e| CoreError::io(&d.path, e))?;
                crate::ports::confine(&rt.scope, fs, &dir)?;
                let owner = session
                    .fresh
                    .skills
                    .iter()
                    .flat_map(|s| &s.deployments)
                    .find(|candidate| {
                        candidate.backing != crate::identity::BackingRelationship::LinkedTo
                            && fs
                                .canonicalize(&candidate.path)
                                .is_ok_and(|path| path == dir)
                    })
                    .unwrap_or(d);
                if crate::ops::plugin_owns_path(rt, &dir)
                    || owner.plugin.is_some()
                    || (owner.mutability != DeploymentMutability::Mutable
                        && !matches!(
                            owner.owner_kind,
                            crate::identity::LifecycleOwnerKind::Manual
                                | crate::identity::LifecycleOwnerKind::InRepo
                        ))
                {
                    return Err(
                        CoreError::new(ErrorCode::Unsupported, "skill is read only").at(&dir)
                    );
                }
                if edits.iter().any(|(path, _, _)| path == &dir) {
                    continue;
                }
                let md = dir.join("SKILL.md");
                let yaml = dir.join("agents/openai.yaml");
                let root = crate::fsops::Root::open(fs, dir.clone())
                    .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
                let mut writes = Vec::new();
                for (path, is_md) in [(md, true), (yaml, false)] {
                    root.confine(
                        path.strip_prefix(&dir).map_err(|e| {
                            CoreError::new(ErrorCode::InvalidRequest, e.to_string())
                        })?,
                    )
                    .map_err(|e| CoreError::new(ErrorCode::InvalidRequest, e.to_string()))?;
                    if fs
                        .symlink_metadata(&path)
                        .is_ok_and(|f| f.kind == crate::ports::FileKind::Symlink)
                    {
                        return Err(CoreError::new(
                            ErrorCode::Unsupported,
                            "invocation file is a symlink; edit its owner",
                        )
                        .at(&path));
                    }
                    let stamp = crate::fsops::read_stamp(fs, &path)
                        .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
                    let bytes = match fs.read_capped(&path, crate::ops::SKILL_MD_MAX_BYTES) {
                        Ok(bytes) => bytes,
                        Err(e) if !is_md && e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                        Err(e) => return Err(CoreError::io(&path, e)),
                    };
                    let text = String::from_utf8(bytes)
                        .map_err(|e| CoreError::new(ErrorCode::InvalidRequest, e.to_string()))?;
                    let updated = if is_md {
                        crate::invocation::rewrite_invocation_frontmatter(&text, policy)
                    } else {
                        crate::invocation::rewrite_codex_yaml(
                            &text,
                            policy == InvocationPolicy::UserOnly,
                        )
                    }
                    .map_err(|e| CoreError::new(ErrorCode::InvalidRequest, e).at(&path))?;
                    writes.push((path, stamp, updated));
                }
                edits.push((dir, owner.clone(), writes));
            }
            let mut ids = Vec::new();
            for (dir, d, writes) in edits {
                let id = rt.ports.ids.next_event_id();
                let manifest =
                    session
                        .store
                        .backup_paths(&session.guard, &id, std::slice::from_ref(&dir))?;
                let fingerprint = manifest
                    .entries
                    .first()
                    .and_then(|e| e.fingerprint.as_ref());
                session.store.record(
                    &session.guard,
                    &id,
                    &crate::events::EventDraft {
                        kind: crate::events::EventKind::InvocationChange,
                        skill: req.skill.clone().ok_or_else(|| {
                            CoreError::new(ErrorCode::InvalidRequest, "choose a skill")
                        })?,
                        harness: d.harness,
                        scope: None,
                        project_path: None,
                        payload: serde_json::json!({"deployment_id":d.id, "policy":policy}),
                        inverse: Some(crate::events::restore_backup_inverse(
                            &dir,
                            fingerprint,
                            None,
                        )),
                        backup_dir: Some(manifest.backup_dir),
                    },
                )?;
                crate::ops_install::ensure_journal_root(rt, &session.guard, fs)?;
                let journal = crate::journal::FsJournal::new(
                    crate::ops_install::journal_root(&rt.scope.home.lexical),
                    rt.ports.fs.clone(),
                );
                let root = crate::fsops::Root::open(fs, dir.clone())
                    .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
                let plan = crate::journal::PlanWriter::begin(
                    &journal,
                    &session.guard,
                    PlanId(id.0.clone()),
                    rt.ports.clock.now(),
                    "invocation",
                    dir.clone(),
                    Vec::new(),
                )?;
                let agents = crate::ports::confine(&rt.scope, fs, &dir.join("agents"))?;
                fs.create_dir_all(&session.guard, &agents)
                    .map_err(|e| CoreError::io(dir.join("agents"), e))?;
                for (path, stamp, text) in writes {
                    crate::fsops::write_file(
                        &root,
                        &plan,
                        path.strip_prefix(&dir).map_err(|e| {
                            CoreError::new(ErrorCode::InvalidRequest, e.to_string())
                        })?,
                        text.as_bytes(),
                        &stamp,
                    )
                    .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?;
                }
                plan.finish(PlanStatus::Done)?;
                let post = crate::events::fingerprint_path(fs, &dir)?;
                session.store.finish(
                    &session.guard,
                    &id,
                    crate::events::EventStatus::Done,
                    post,
                )?;
                ids.push(id);
            }
            session.finish(rt, ctx);
            ids
        } else {
            Vec::new()
        };
        let inventory = crate::ops::scan(
            rt,
            ctx,
            &crate::dto::ScanRequest {
                skills: req.skill.iter().cloned().collect(),
                timings: false,
            },
        )?;
        let mut report = InvocationReport {
            skills: Vec::new(),
            event_ids,
        };
        for skill in inventory.skills {
            let mut rows = Vec::new();
            let mut modes = Vec::new();
            for d in skill.deployments {
                let manual = d
                    .frontmatter
                    .as_ref()
                    .is_some_and(|f| f.disable_model_invocation == Some(true));
                let opencode_manual = d
                    .frontmatter
                    .as_ref()
                    .and_then(|f| f.metadata.as_ref())
                    .and_then(|m| m.get("opencode/autoinvoke"))
                    .and_then(|v| {
                        v.as_bool()
                            .or_else(|| v.as_i64().filter(|n| matches!(n, 0 | 1)).map(|n| n == 1))
                    })
                    .map_or(manual, |auto| !auto);
                let bytes = match rt.ports.fs.read_capped(
                    &d.path.join("agents/openai.yaml"),
                    crate::ops::SKILL_MD_MAX_BYTES,
                ) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(CoreError::io(&d.path, e)),
                };
                let codex: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(|e| {
                    CoreError::new(ErrorCode::InvalidRequest, e.to_string()).at(&d.path)
                })?;
                let codex_manual = codex
                    .get("policy")
                    .and_then(|p| p.get("allow_implicit_invocation"))
                    .and_then(serde_yaml::Value::as_bool)
                    == Some(false);
                // Compare the two skill-owned controls even for a single copy.
                modes.extend([manual, codex_manual]);
                let harnesses = d.harness.clone().map_or_else(
                    || {
                        rt.ports
                            .catalog
                            .facts
                            .iter()
                            .filter(|facts| facts.reads_universal_root.is_yes())
                            .map(|h| h.id.clone())
                            .collect()
                    },
                    |h| vec![h],
                );
                for harness in harnesses {
                    let mode = match harness.as_str() {
                        AgentId::CLAUDE_CODE | AgentId::PI | AgentId::CURSOR => {
                            if manual {
                                InvocationMode::Manual
                            } else {
                                InvocationMode::Auto
                            }
                        }
                        AgentId::OPEN_CODE => {
                            modes.push(opencode_manual);
                            if opencode_manual {
                                InvocationMode::Manual
                            } else {
                                InvocationMode::Auto
                            }
                        }
                        AgentId::CODEX => {
                            if codex_manual {
                                InvocationMode::Manual
                            } else {
                                InvocationMode::Auto
                            }
                        }
                        _ => InvocationMode::Unsupported,
                    };
                    rows.push(InvocationDeployment {
                        deployment_id: d.id.clone(),
                        harness,
                        path: d.path.clone(),
                        mode,
                    });
                }
            }
            report.skills.push(SkillInvocationReport {
                skill: skill.name,
                deployments: rows,
                mismatch: modes.iter().any(|m| *m) && modes.iter().any(|m| !*m),
            });
        }
        Ok(report)
    })
}
