//! The programs and access an owner-tool install needs, and the one plain
//! sentence a person sees when one is missing. Single-skill install,
//! update, remove and the pack actions all check here, so each path says
//! the same thing.

use crate::error::{CoreError, ErrorCode};
use crate::ports::Runtime;

/// Shown when `npx skills` or dotagents cannot run.
pub const NODE_MISSING: &str =
    "Node.js is not installed. Install it from nodejs.org, then try again.";

/// Shown when git cannot run. On macOS, git comes with Apple's command line
/// developer tools.
pub const GIT_MISSING: &str = "Git is not installed. Install Apple's command line developer tools \
     (run xcode-select --install in Terminal), then try again.";

/// Shown when git cannot read `source`: it is private, or it does not exist.
pub fn no_access(source: &str) -> String {
    format!(
        "You don't have access to {source}, or it does not exist. Ask its owner for access, \
         sign in to GitHub on this Mac (GitHub Desktop or gh auth login), then try again."
    )
}

/// True when the host's tool lookup finds `npx`, which Node.js installs.
/// False when the host has no tool lookup.
pub(crate) fn node_on_path(rt: &Runtime) -> bool {
    rt.ports
        .tools
        .as_ref()
        .is_some_and(|tools| tools.find_binary("npx").is_some())
}

/// Fails with [`NODE_MISSING`] when the host's tool lookup finds no Node.
/// Call it before anything runs `npx`. A host with no tool lookup is not
/// checked: its process spawner reports instead.
pub(crate) fn require_node(rt: &Runtime) -> Result<(), CoreError> {
    if rt.ports.tools.is_none() || node_on_path(rt) {
        return Ok(());
    }
    Err(CoreError::new(ErrorCode::Unsupported, NODE_MISSING))
}
