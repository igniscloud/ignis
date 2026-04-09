use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::SkillFormat;
use crate::output;
use crate::skill_bundle;

pub fn generate(format: SkillFormat, path: Option<&Path>, force: bool) -> Result<()> {
    let root = gen_skill_output_root(format, path);
    if root.exists() {
        if !force {
            bail!(
                "{} already exists; pass --force to overwrite",
                root.display()
            );
        }
        if root.is_dir() {
            fs::remove_dir_all(&root).with_context(|| format!("removing {}", root.display()))?;
        } else {
            fs::remove_file(&root).with_context(|| format!("removing {}", root.display()))?;
        }
    }

    match format {
        SkillFormat::Codex | SkillFormat::Opencode => {
            write_bundled_skill_dir(&root, "SKILL.md", skill_bundle::ignis_user_files())?;
        }
        SkillFormat::Raw => {
            fs::create_dir_all(root.join("references"))
                .with_context(|| format!("creating {}", root.join("references").display()))?;
            for file in skill_bundle::ignis_user_files() {
                if file.path == "SKILL.md" {
                    continue;
                }
                let destination = root.join(file.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                fs::write(&destination, file.contents)
                    .with_context(|| format!("writing {}", destination.display()))?;
            }
            let entrypoint = root.join("skill.md");
            fs::write(&entrypoint, skill_bundle::raw_ignis_user_markdown())
                .with_context(|| format!("writing {}", entrypoint.display()))?;
        }
    }

    output::success(serde_json::json!({
        "format": skill_format_name(format),
        "name": "ignis-user",
        "path": root,
        "entrypoint": skill_entrypoint(format, &root),
    }))
}

fn skill_entrypoint(format: SkillFormat, root: &Path) -> PathBuf {
    match format {
        SkillFormat::Codex | SkillFormat::Opencode => root.join("SKILL.md"),
        SkillFormat::Raw => root.join("skill.md"),
    }
}

fn gen_skill_output_root(format: SkillFormat, path: Option<&Path>) -> PathBuf {
    if let Some(path) = path {
        return path.to_path_buf();
    }
    match format {
        SkillFormat::Codex => PathBuf::from(".codex").join("skills").join("ignis-user"),
        SkillFormat::Opencode => PathBuf::from(".opencode").join("skills").join("ignis-user"),
        SkillFormat::Raw => PathBuf::from("ignis-user"),
    }
}

fn skill_format_name(format: SkillFormat) -> &'static str {
    match format {
        SkillFormat::Codex => "codex",
        SkillFormat::Opencode => "opencode",
        SkillFormat::Raw => "raw",
    }
}

fn write_bundled_skill_dir(
    root: &Path,
    entrypoint_name: &str,
    files: &[skill_bundle::BundledFile],
) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    for file in files {
        let relative_path = if file.path == "SKILL.md" {
            PathBuf::from(entrypoint_name)
        } else {
            PathBuf::from(file.path)
        };
        let destination = root.join(relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&destination, file.contents)
            .with_context(|| format!("writing {}", destination.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::gen_skill_output_root;
    use crate::cli::SkillFormat;

    #[test]
    fn gen_skill_output_root_uses_format_defaults() {
        assert_eq!(
            gen_skill_output_root(SkillFormat::Codex, None),
            Path::new(".codex").join("skills").join("ignis-user")
        );
        assert_eq!(
            gen_skill_output_root(SkillFormat::Opencode, None),
            Path::new(".opencode").join("skills").join("ignis-user")
        );
        assert_eq!(
            gen_skill_output_root(SkillFormat::Raw, None),
            Path::new("ignis-user")
        );
    }
}
