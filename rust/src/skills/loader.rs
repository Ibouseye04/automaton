//! Skill system — discovers and loads SKILL.md files.
//!
//! Skills are user-defined capabilities stored as Markdown files with
//! YAML frontmatter (name, description, version, auto_activate, requirements).

use crate::types::{Skill, SkillRequirement};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info, warn};

/// Load all skills from the skills directory.
pub fn load_skills(skills_dir: &str) -> Result<Vec<Skill>> {
    let dir = Path::new(skills_dir);

    if !dir.exists() {
        debug!("Skills directory does not exist: {:?}", dir);
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    let entries = std::fs::read_dir(dir).context("Failed to read skills directory")?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Look for SKILL.md files (either directly or in subdirectories)
        if path.is_file() && path.file_name().map(|n| n == "SKILL.md").unwrap_or(false) {
            match parse_skill_file(&path) {
                Ok(skill) => {
                    info!("Loaded skill: {} v{}", skill.name, skill.version);
                    skills.push(skill);
                }
                Err(e) => {
                    warn!("Failed to parse skill at {:?}: {}", path, e);
                }
            }
        } else if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                match parse_skill_file(&skill_file) {
                    Ok(skill) => {
                        info!("Loaded skill: {} v{}", skill.name, skill.version);
                        skills.push(skill);
                    }
                    Err(e) => {
                        warn!("Failed to parse skill at {:?}: {}", skill_file, e);
                    }
                }
            }
        }
    }

    info!("Loaded {} skills from {:?}", skills.len(), dir);
    Ok(skills)
}

/// YAML frontmatter structure for a SKILL.md file.
#[derive(Debug, serde::Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    auto_activate: Option<bool>,
    #[serde(default)]
    requirements: Vec<SkillReqYaml>,
}

#[derive(Debug, serde::Deserialize)]
struct SkillReqYaml {
    #[serde(rename = "type")]
    kind: String,
    value: String,
}

/// Parse a SKILL.md file with YAML frontmatter.
///
/// Format:
/// ```markdown
/// ---
/// name: my-skill
/// description: Does something
/// version: 1.0.0
/// auto_activate: true
/// ---
/// Instructions here...
/// ```
fn parse_skill_file(path: &Path) -> Result<Skill> {
    let content = std::fs::read_to_string(path).context("Failed to read skill file")?;

    // Split frontmatter from content manually
    let (frontmatter_str, instructions) = split_frontmatter(&content);

    let fm: SkillFrontmatter = if frontmatter_str.is_empty() {
        SkillFrontmatter {
            name: None,
            description: None,
            version: None,
            auto_activate: None,
            requirements: Vec::new(),
        }
    } else {
        serde_yaml::from_str(frontmatter_str).context("Failed to parse SKILL.md frontmatter")?
    };

    let default_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    Ok(Skill {
        name: fm.name.unwrap_or(default_name),
        description: fm.description.unwrap_or_default(),
        version: fm.version.unwrap_or_else(|| "1.0.0".to_string()),
        auto_activate: fm.auto_activate.unwrap_or(false),
        instructions,
        requirements: fm
            .requirements
            .into_iter()
            .map(|r| SkillRequirement {
                kind: r.kind,
                value: r.value,
            })
            .collect(),
    })
}

/// Split YAML frontmatter (between `---` markers) from the rest of the content.
fn split_frontmatter(content: &str) -> (&str, String) {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return ("", content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    if let Some(end_idx) = after_first.find("\n---") {
        let fm = &after_first[..end_idx].trim();
        let body = &after_first[end_idx + 4..];
        (fm, body.trim_start_matches('\n').to_string())
    } else {
        ("", content.to_string())
    }
}
