use serde_json::json;

use crate::project::ProjectRegisterParams;

#[test]
fn project_register_params_use_the_shared_camel_case_contract() {
    let params: ProjectRegisterParams = serde_json::from_value(json!({
        "root": "/workspace/app",
        "label": "Application"
    }))
    .unwrap();

    assert_eq!(params.root, "/workspace/app");
    assert_eq!(params.label.as_deref(), Some("Application"));
    assert_eq!(
        serde_json::to_value(params).unwrap(),
        json!({ "root": "/workspace/app", "label": "Application" })
    );
}
