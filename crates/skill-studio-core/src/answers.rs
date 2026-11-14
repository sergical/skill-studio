//! Short answers built from the full results: one row per skill, no paths
//! and no ids, so an agent reads the answer instead of filtering raw data.
//! The CLI (`--short`) and the MCP server return the same shapes.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{Completeness, CopyStatus, DeploymentDto, InstalledSkillDto, Inventory};
use crate::identity::{AgentId, RootKind};
use crate::ops::{OpStatus, Outcome};
use crate::ops_invocation::{InvocationMode, InvocationReport};

/// A scan with one row per skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScanSummary {
    /// Skills in name order.
    pub skills: Vec<SkillSummary>,
    /// Set when a root could not be read, so a skill may be missing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

/// One skill in a [`ScanSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SkillSummary {
    /// Skill name.
    pub name: String,
    /// Owner: `owner/repo`, `plugin@marketplace`, or `manual`.
    pub source: String,
    /// Agents whose own skills folder holds a copy or a link, by id.
    pub agents: BTreeSet<AgentId>,
    /// Set when the shared library (`.agents/skills`) holds a copy.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    /// Copy status of each agent copy against the library copy, when known.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub copies: BTreeMap<AgentId, CopyStatus>,
    /// Set when every copy of the skill is parked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub parked: bool,
}

impl Outcome for ScanSummary {
    fn status(&self) -> OpStatus {
        if self.partial {
            OpStatus::Partial
        } else {
            OpStatus::Ok
        }
    }
}

fn active(skill: &InstalledSkillDto) -> impl Iterator<Item = &DeploymentDto> {
    skill
        .deployments
        .iter()
        .filter(|d| d.root.kind != RootKind::Parked)
}

/// Shortens a scan to one row per skill.
pub fn summarize_scan(inventory: &Inventory) -> ScanSummary {
    let mut skills: Vec<SkillSummary> = inventory
        .skills
        .iter()
        .map(|skill| SkillSummary {
            name: skill.name.0.clone(),
            source: skill.source.clone(),
            agents: active(skill).filter_map(|d| d.harness.clone()).collect(),
            library: active(skill).any(|d| d.root.kind == RootKind::Universal),
            copies: skill
                .deployments
                .iter()
                .filter_map(|d| Some((d.harness.clone()?, d.copy_status?)))
                .collect(),
            parked: skill
                .deployments
                .iter()
                .all(|d| d.root.kind == RootKind::Parked),
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    ScanSummary {
        skills,
        partial: inventory.completeness == Completeness::Partial,
    }
}

/// Skills with one copy read as manual and another as automatic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InvocationMismatches {
    /// Skills in name order.
    pub skills: Vec<InvocationMismatch>,
}

/// One skill in [`InvocationMismatches`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InvocationMismatch {
    /// Skill name.
    pub skill: String,
    /// Agents that read a copy as manual only.
    pub manual: BTreeSet<AgentId>,
    /// Agents that read a copy as automatic.
    pub auto: BTreeSet<AgentId>,
}

impl Outcome for InvocationMismatches {}

/// Keeps only the skills whose agents disagree on invocation.
pub fn invocation_mismatches(report: &InvocationReport) -> InvocationMismatches {
    let mut skills: Vec<InvocationMismatch> = report
        .skills
        .iter()
        .filter(|skill| skill.mismatch)
        .map(|skill| {
            let agents = |mode: InvocationMode| {
                skill
                    .deployments
                    .iter()
                    .filter(|d| d.mode == mode)
                    .map(|d| d.harness.clone())
                    .collect()
            };
            InvocationMismatch {
                skill: skill.skill.0.clone(),
                manual: agents(InvocationMode::Manual),
                auto: agents(InvocationMode::Auto),
            }
        })
        .collect();
    skills.sort_by(|a, b| a.skill.cmp(&b.skill));
    InvocationMismatches { skills }
}
