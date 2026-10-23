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

/// Language of the role prompt. English is the default; `zh` loads the
/// `roles/*_zh.md` translation. Tool names and JSON contracts are identical.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptLang {
    #[default]
    En,
    Zh,
}

const DESIGNER_EN: &str = include_str!("../../roles/designer.md");
const DESIGNER_ZH: &str = include_str!("../../roles/designer_zh.md");
const ARTIST_EN: &str = include_str!("../../roles/artist.md");
const ARTIST_ZH: &str = include_str!("../../roles/artist_zh.md");

/// The designer produces the design package. It cannot touch pxl or approve
/// itself. v3: the structured todo list is granted (self-planning aid);
/// fs tools stay off — writes must go through design.write_candidate.
pub fn designer() -> RoleProfile {
    designer_for(PromptLang::En)
}

pub fn designer_for(lang: PromptLang) -> RoleProfile {
    let prompt = match lang {
        PromptLang::En => DESIGNER_EN,
        PromptLang::Zh => DESIGNER_ZH,
    };
    RoleProfile {
        name: "designer".into(),
        prompt: prompt.to_string(),
        tools: vec![
            "design.read_brief".into(),
            "design.read_template".into(),
            "design.write_candidate".into(),
            "design.validate".into(),
            "design.request_review".into(),
            "todo.write".into(),
            "todo.read".into(),
        ],
        permissions: vec![
            Permission::ReadProject,
            Permission::WriteCandidate,
            Permission::Todo,
        ],
    }
}

/// The artist turns frozen requirements into recipes and builds them.
/// v3: todo granted; fs tools stay off — asset writes must go through
/// assets.write_candidate so the pending-check queue stays authoritative.
pub fn artist() -> RoleProfile {
    artist_for(PromptLang::En)
}

pub fn artist_for(lang: PromptLang) -> RoleProfile {
    let prompt = match lang {
        PromptLang::En => ARTIST_EN,
        PromptLang::Zh => ARTIST_ZH,
    };
    RoleProfile {
        name: "artist".into(),
        prompt: prompt.to_string(),
        tools: vec![
            "design.read_frozen".into(),
            "assets.read_requirements".into(),
            "assets.read_candidate".into(),
            "assets.write_candidate".into(),
            "assets.build_candidate".into(),
            "assets.validate_candidate".into(),
            "assets.list_capabilities".into(),
            "assets.schema".into(),
            "assets.status".into(),
            "assets.request_publish".into(),
            "todo.write".into(),
            "todo.read".into(),
        ],
        permissions: vec![
            Permission::ReadProject,
            Permission::WriteCandidate,
            Permission::Pxl,
            Permission::Todo,
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

    #[test]
    fn prompt_lang_selects_translation_with_same_tools() {
        let en = artist_for(PromptLang::En);
        let zh = artist_for(PromptLang::Zh);
        assert_ne!(en.prompt, zh.prompt);
        assert!(zh.prompt.contains("美术"));
        assert_eq!(en.tools, zh.tools);
        let d_zh = designer_for(PromptLang::Zh);
        assert!(d_zh.prompt.contains("设计"));
        assert_eq!(designer().tools, d_zh.tools);
    }

    #[test]
    fn artist_can_reach_every_tool_its_prompt_names() {
        let a = artist();
        for tool in ["assets.list_capabilities", "assets.schema", "assets.status"] {
            assert!(a.prompt.contains(tool), "prompt should mention {tool}");
            assert!(a.tools.iter().any(|t| t == tool), "artist tools miss {tool}");
        }
    }
}
