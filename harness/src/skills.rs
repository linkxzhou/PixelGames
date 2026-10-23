//! On-disk skills. A skill is a directory under `harness/skills/<name>/`
//! with a `SKILL.md` whose frontmatter names it. Pipeline stages load skills
//! by that name; a missing file is a failed gate, not a silent default.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{HarnessError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
    pub path: PathBuf,
}

pub fn bundled_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills")
}

pub fn load_dir(dir: &Path) -> Result<Vec<Skill>> {
    if !dir.is_dir() {
        return Err(HarnessError::not_found(format!(
            "skill directory {} is missing",
            dir.display()
        )));
    }
    let mut skills = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for entry in entries {
        let path = entry.join("SKILL.md");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        skills.push(parse_skill(&path, &text)?);
    }
    Ok(skills)
}

pub fn load_bundled() -> Result<Vec<Skill>> {
    load_dir(&bundled_dir())
}

pub fn require(name: &str) -> Result<Skill> {
    load_bundled()?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| {
            HarnessError::not_found(format!("skill '{name}' is not loadable"))
                .with_hint("expected harness/skills/<name>/SKILL.md")
        })
}

fn parse_skill(path: &Path, text: &str) -> Result<Skill> {
    let (front, body) = split_frontmatter(text).ok_or_else(|| {
        HarnessError::schema(format!("{} is missing frontmatter", path.display()))
            .with_hint("start SKILL.md with a --- name/description block")
    })?;
    let mut name = String::new();
    let mut description = String::new();
    for line in front.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            name = value.trim().trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("description:") {
            description = value.trim().trim_matches('"').to_string();
        }
    }
    if name.is_empty() {
        name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("skill")
            .to_string();
    }
    if description.is_empty() {
        return Err(HarnessError::schema(format!(
            "{} has no description",
            path.display()
        )));
    }
    Ok(Skill {
        name,
        description,
        body: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))?;
    let end = rest.find("\n---\n").or_else(|| rest.find("\n---\r\n"))?;
    let front = &rest[..end];
    let body = &rest[end + 1..];
    let body = body.trim_start_matches("---").trim_start_matches('\r').trim_start_matches('\n');
    Some((front, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macroquad_skill_is_loadable() {
        let skills = load_bundled().unwrap();
        let skill = skills.iter().find(|s| s.name == "macroquad").expect("macroquad skill");
        assert!(skill.description.to_lowercase().contains("macroquad"));
        assert!(skill.body.contains("FilterMode::Nearest"));
        assert!(skill.body.contains("--headless"));
        assert!(skill.body.contains("wasm32-unknown-unknown"));
        assert_eq!(require("macroquad").unwrap().name, "macroquad");
        assert!(require("missing-skill").is_err());
    }
}
