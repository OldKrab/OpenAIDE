use std::fs;

use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::settings::{
    SettingsProjectionAvailability, SettingsScope, SettingsSkillDetailsParams, SettingsSkillsParams,
};
use tempfile::tempdir;

use super::{SkillsSettingsService, SkillsSettingsWorkflow};
use crate::projects::ConfiguredProjectRoots;

#[test]
fn discovers_user_and_workspace_skills_without_exposing_paths() {
    let sandbox = tempdir().expect("sandbox");
    let user_skills = sandbox.path().join("user-skills");
    let workspace = sandbox.path().join("workspace");
    write_skill(
        &user_skills.join("review"),
        "---\nname: Review\ndescription: Review changes\n---\nFollow the checklist.\n",
    );
    write_skill(
        &workspace.join(".agents/skills/research"),
        "---\nname: Research\ndescription: Find primary sources\n---\nCite the findings.\n",
    );
    let service = SkillsSettingsService::with_roots(
        Some(user_skills),
        ConfiguredProjectRoots::from_workspace_roots([workspace.to_string_lossy().to_string()]),
    );

    let result = service
        .skills_settings(SettingsSkillsParams {})
        .expect("skills projection");

    assert_eq!(
        result.availability,
        SettingsProjectionAvailability::Available
    );
    assert_eq!(result.skills.len(), 2);
    assert_eq!(result.skills[0].scope, SettingsScope::Global);
    assert_eq!(result.skills[0].label, "Review");
    assert_eq!(result.skills[1].scope, SettingsScope::Workspace);
    assert_eq!(result.skills[1].label, "Research");
    for skill in result.skills {
        assert!(!skill.id.contains(sandbox.path().to_string_lossy().as_ref()));
    }
}

#[test]
fn skill_details_preserve_required_fields_arbitrary_metadata_instructions_and_source() {
    let sandbox = tempdir().expect("sandbox");
    let user_skills = sandbox.path().join("user-skills");
    let source = "---\nname: Research\ndescription: Find primary sources\ncompatibility:\n  tools:\n    - web\n    - files\nlabels: [careful, cited]\n---\n# Workflow\n\nRead every source.\n";
    write_skill(&user_skills.join("research"), source);
    let service =
        SkillsSettingsService::with_roots(Some(user_skills), ConfiguredProjectRoots::default());
    let listed = service
        .skills_settings(SettingsSkillsParams {})
        .expect("skills projection");

    let result = service
        .skill_details(SettingsSkillDetailsParams {
            id: listed.skills[0].id.clone(),
        })
        .expect("skill details");

    assert_eq!(result.document.name, "Research");
    assert_eq!(result.document.description, "Find primary sources");
    assert_eq!(
        result
            .document
            .additional_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["compatibility", "labels"]
    );
    assert!(result.document.additional_fields[0].value.contains("- web"));
    assert!(result.document.instructions.contains("# Workflow"));
    assert_eq!(result.document.source, source);
}

#[test]
fn skill_details_reject_an_unknown_opaque_id() {
    let service = SkillsSettingsService::with_roots(None, ConfiguredProjectRoots::default());

    let error = service
        .skill_details(SettingsSkillDetailsParams {
            id: "skill-missing".to_string(),
        })
        .expect_err("unknown id should fail");

    assert_eq!(error.code, ProtocolErrorCode::NotFound);
}

fn write_skill(directory: &std::path::Path, source: &str) {
    fs::create_dir_all(directory).expect("skill directory");
    fs::write(directory.join("SKILL.md"), source).expect("skill source");
}
