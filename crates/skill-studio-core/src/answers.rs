//! Compact invocation mismatch answers shared by CLI and MCP.
use crate::identity::AgentId;
use crate::ops::Outcome;
use crate::ops_invocation::{InvocationMode, InvocationReport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
