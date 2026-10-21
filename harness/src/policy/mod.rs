//! Permission model. The effective tool set is
//! host policy ∩ stage permissions ∩ role request.

pub mod budget;
pub mod path;

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read files inside the project and approved template roots.
    ReadProject,
    /// Write candidate files inside the current attempt directory.
    WriteCandidate,
    /// Call the approved `pxl` binary through the adapter.
    Pxl,
    /// Reserved for the future engine stage.
    Engine,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionSet(pub Vec<Permission>);

impl PermissionSet {
    pub fn allow(mut self, p: Permission) -> Self {
        if !self.0.contains(&p) {
            self.0.push(p);
        }
        self
    }
    pub fn has(&self, p: &Permission) -> bool {
        self.0.contains(p)
    }
    pub fn intersect(&self, other: &PermissionSet) -> PermissionSet {
        PermissionSet(self.0.iter().filter(|p| other.has(p)).cloned().collect())
    }
}

pub fn check_tool_allowed(
    host: &PermissionSet,
    stage: &PermissionSet,
    role: &PermissionSet,
    needed: &Permission,
) -> Result<()> {
    if host.has(needed) && stage.has(needed) && role.has(needed) {
        return Ok(());
    }
    Err(HarnessError::policy(format!(
        "tool requires {:?} permission which is not granted by host/stage/role",
        needed
    ))
    .with_hint("the coordinator narrows permissions; roles cannot widen them"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_narrows() {
        let host = PermissionSet::default().allow(Permission::Pxl);
        let stage = PermissionSet::default().allow(Permission::Pxl);
        let role = PermissionSet::default().allow(Permission::ReadProject);
        assert!(check_tool_allowed(&host, &stage, &role, &Permission::Pxl).is_err());
        let role2 = PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::Pxl);
        assert!(check_tool_allowed(&host, &stage, &role2, &Permission::Pxl).is_ok());
        assert!(host.intersect(&stage).has(&Permission::Pxl));
    }
}
