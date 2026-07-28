use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::settings::{
    SettingsProjectionAvailability, SettingsProjectionNotice, SettingsProjectionNoticeSeverity,
    SettingsScope, SettingsSkillDetailsParams, SettingsSkillDetailsResult, SettingsSkillDocument,
    SettingsSkillDocumentField, SettingsSkillRecord, SettingsSkillStatus, SettingsSkillsParams,
    SettingsSkillsResult,
};
use serde_yaml::{Mapping, Value};
use uuid::Uuid;

use crate::projects::ConfiguredProjectRoots;
use crate::time::now_string;

pub(crate) trait SkillsSettingsWorkflow: Send + Sync {
    fn skills_settings(
        &self,
        params: SettingsSkillsParams,
    ) -> Result<SettingsSkillsResult, ProtocolError>;

    fn skill_details(
        &self,
        params: SettingsSkillDetailsParams,
    ) -> Result<SettingsSkillDetailsResult, ProtocolError>;
}

#[derive(Clone)]
pub(crate) struct SkillsSettingsService {
    global_skills_root: Option<PathBuf>,
    project_roots: ConfiguredProjectRoots,
    catalog: Arc<RwLock<SkillCatalog>>,
}

#[derive(Default)]
struct SkillCatalog {
    ids_by_path: HashMap<PathBuf, String>,
    skills_by_id: HashMap<String, IndexedSkill>,
}

#[derive(Clone)]
struct IndexedSkill {
    source_path: PathBuf,
    record: SettingsSkillRecord,
}

struct SkillSearchRoot {
    directory: PathBuf,
    scope: SettingsScope,
    source_label: String,
}

impl SkillsSettingsService {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_roots(
            default_global_skills_root(),
            ConfiguredProjectRoots::default(),
        )
    }

    pub(crate) fn with_project_roots(project_roots: ConfiguredProjectRoots) -> Self {
        Self::with_roots(default_global_skills_root(), project_roots)
    }

    pub(crate) fn with_roots(
        global_skills_root: Option<PathBuf>,
        project_roots: ConfiguredProjectRoots,
    ) -> Self {
        Self {
            global_skills_root,
            project_roots,
            catalog: Arc::new(RwLock::new(SkillCatalog::default())),
        }
    }

    fn search_roots(&self) -> Vec<SkillSearchRoot> {
        let mut roots = Vec::new();
        if let Some(directory) = &self.global_skills_root {
            roots.push(SkillSearchRoot {
                directory: directory.clone(),
                scope: SettingsScope::Global,
                source_label: "User configuration".to_string(),
            });
        }
        roots.extend(
            self.project_roots
                .projects()
                .into_iter()
                .map(|project| SkillSearchRoot {
                    directory: PathBuf::from(project.workspace_root).join(".agents/skills"),
                    scope: SettingsScope::Workspace,
                    source_label: project.label,
                }),
        );
        roots
    }

    fn scan(&self) -> (Vec<SettingsSkillRecord>, Vec<SettingsProjectionNotice>) {
        let scanned_at = now_string();
        let mut catalog = self.catalog.write().expect("skills catalog lock poisoned");
        let mut next_by_id = HashMap::new();
        let mut records = Vec::new();
        let mut notices = Vec::new();

        for root in self.search_roots() {
            let directories = match skill_directories(&root.directory) {
                Ok(directories) => directories,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    notices.push(SettingsProjectionNotice {
                        severity: SettingsProjectionNoticeSeverity::Warning,
                        message: format!("Could not scan skills from {}.", root.source_label),
                    });
                    continue;
                }
            };
            for directory in directories {
                let source_path = directory.join("SKILL.md");
                if !source_path.is_file() {
                    continue;
                }
                let id = catalog
                    .ids_by_path
                    .entry(source_path.clone())
                    .or_insert_with(|| format!("skill-{}", Uuid::new_v4()))
                    .clone();
                let record = list_record(&source_path, &directory, &root, &id, &scanned_at);
                records.push(record.clone());
                next_by_id.insert(
                    id,
                    IndexedSkill {
                        source_path,
                        record,
                    },
                );
            }
        }

        catalog
            .ids_by_path
            .retain(|_, id| next_by_id.contains_key(id));
        catalog.skills_by_id = next_by_id;
        records.sort_by(|left, right| {
            scope_rank(left.scope)
                .cmp(&scope_rank(right.scope))
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.label.cmp(&right.label))
        });
        (records, notices)
    }
}

impl SkillsSettingsWorkflow for SkillsSettingsService {
    fn skills_settings(
        &self,
        _params: SettingsSkillsParams,
    ) -> Result<SettingsSkillsResult, ProtocolError> {
        let (skills, notices) = self.scan();
        Ok(SettingsSkillsResult {
            generated_at: now_string(),
            availability: SettingsProjectionAvailability::Available,
            skills,
            notices,
        })
    }

    fn skill_details(
        &self,
        params: SettingsSkillDetailsParams,
    ) -> Result<SettingsSkillDetailsResult, ProtocolError> {
        let indexed = self
            .catalog
            .read()
            .expect("skills catalog lock poisoned")
            .skills_by_id
            .get(&params.id)
            .cloned()
            .ok_or_else(skill_not_found)?;
        let source = fs::read_to_string(&indexed.source_path).map_err(|_| ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: "The skill file could not be read.".to_string(),
            recoverable: true,
            target: None,
        })?;
        let document = parse_skill_document(&source).map_err(|message| ProtocolError {
            code: ProtocolErrorCode::ValidationFailed,
            message,
            recoverable: true,
            target: None,
        })?;
        Ok(SettingsSkillDetailsResult {
            generated_at: now_string(),
            skill: indexed.record,
            document,
        })
    }
}

fn default_global_skills_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".agents/skills"))
}

fn skill_directories(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut directories = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir() || kind.is_symlink())
                .map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    directories.sort();
    Ok(directories)
}

fn list_record(
    source_path: &Path,
    directory: &Path,
    root: &SkillSearchRoot,
    id: &str,
    scanned_at: &str,
) -> SettingsSkillRecord {
    let fallback_label = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unnamed skill")
        .to_string();
    match fs::read_to_string(source_path)
        .map_err(|_| "The skill file could not be read.".to_string())
        .and_then(|source| parse_skill_document(&source))
    {
        Ok(document) => SettingsSkillRecord {
            id: id.to_string(),
            label: document.name,
            scope: root.scope,
            source_label: root.source_label.clone(),
            status: SettingsSkillStatus::Valid,
            description: Some(document.description),
            warnings: Vec::new(),
            tags: Vec::new(),
            last_scanned_at: scanned_at.to_string(),
        },
        Err(message) => SettingsSkillRecord {
            id: id.to_string(),
            label: fallback_label,
            scope: root.scope,
            source_label: root.source_label.clone(),
            status: SettingsSkillStatus::Invalid,
            description: None,
            warnings: vec![message],
            tags: Vec::new(),
            last_scanned_at: scanned_at.to_string(),
        },
    }
}

fn parse_skill_document(source: &str) -> Result<SettingsSkillDocument, String> {
    let (frontmatter, instructions) = split_frontmatter(source)?;
    let metadata = serde_yaml::from_str::<Mapping>(frontmatter)
        .map_err(|error| format!("Invalid skill frontmatter: {error}"))?;
    let name = required_string(&metadata, "name")?;
    let description = required_string(&metadata, "description")?;
    let additional_fields = metadata
        .iter()
        .filter_map(|(key, value)| {
            let name = yaml_key(key);
            (!matches!(name.as_str(), "name" | "description")).then(|| {
                serde_yaml::to_string(value)
                    .map(|value| SettingsSkillDocumentField {
                        name,
                        value: value.trim_end().to_string(),
                    })
                    .map_err(|error| format!("Could not render skill metadata: {error}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SettingsSkillDocument {
        name,
        description,
        additional_fields,
        instructions: instructions.to_string(),
        source: source.to_string(),
    })
}

fn split_frontmatter(source: &str) -> Result<(&str, &str), String> {
    let mut offset = 0;
    let mut lines = source.split_inclusive('\n');
    let first = lines
        .next()
        .ok_or_else(|| "Skill file is empty.".to_string())?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return Err("Skill file must begin with YAML frontmatter.".to_string());
    }
    offset += first.len();
    let frontmatter_start = offset;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let frontmatter_end = offset;
            offset += line.len();
            return Ok((
                &source[frontmatter_start..frontmatter_end],
                &source[offset..],
            ));
        }
        offset += line.len();
    }
    Err("Skill frontmatter is missing its closing delimiter.".to_string())
}

fn required_string(metadata: &Mapping, field: &str) -> Result<String, String> {
    metadata
        .get(Value::String(field.to_string()))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Skill frontmatter requires a non-empty `{field}` field."))
}

fn yaml_key(value: &Value) -> String {
    value.as_str().map(str::to_string).unwrap_or_else(|| {
        serde_yaml::to_string(value)
            .unwrap_or_else(|_| "<unsupported key>".to_string())
            .trim()
            .to_string()
    })
}

fn scope_rank(scope: SettingsScope) -> u8 {
    match scope {
        SettingsScope::Global => 0,
        SettingsScope::Workspace => 1,
    }
}

fn skill_not_found() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: "The requested skill is no longer available. Refresh the skills list.".to_string(),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
