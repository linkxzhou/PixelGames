//! Role profiles. Roles narrow permissions and tool sets; they never widen
//! host or stage permissions.

use serde::{Deserialize, Serialize};

use crate::policy::{Permission, PermissionSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleProfile {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

impl RoleProfile {
    pub fn permission_set(&self) -> PermissionSet {
        let mut set = PermissionSet::default();
        for p in &self.permissions {
            set = set.allow(p.clone());
        }
        set
    }
}

/// The designer produces the design package. It cannot touch pxl or approve
/// itself.
pub fn designer() -> RoleProfile {
    RoleProfile {
        name: "designer".into(),
        prompt: include_str!("../../roles/designer.md").to_string(),
        tools: vec![
            "design.read_brief".into(),
            "design.read_template".into(),
            "design.write_candidate".into(),
            "design.validate".into(),
            "design.request_review".into(),
        ],
        permissions: vec![Permission::ReadProject, Permission::WriteCandidate],
    }
}

/// The artist turns frozen requirements into recipes and builds them.
pub fn artist() -> RoleProfile {
    RoleProfile {
        name: "artist".into(),
        prompt: include_str!("../../roles/artist.md").to_string(),
        tools: vec![
            "design.read_frozen".into(),
            "assets.read_requirements".into(),
            "assets.write_candidate".into(),
            "assets.build_candidate".into(),
            "assets.validate_candidate".into(),
            "assets.list_capabilities".into(),
            "assets.request_publish".into(),
        ],
        permissions: vec![
            Permission::ReadProject,
            Permission::WriteCandidate,
            Permission::Pxl,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_narrow_permissions() {
        let d = designer();
        assert!(!d.permission_set().has(&Permission::Pxl));
        let a = artist();
        assert!(a.permission_set().has(&Permission::Pxl));
        // Neither role may touch the reserved engine permission.
        assert!(!a.permission_set().has(&Permission::Engine));
    }
}
