use serde_json::json;

use super::*;

#[test]
fn client_probe_result_uses_endpoint_validation_facts() {
    let result = ClientProbeResult {
        state_root_fingerprint: "root-a".to_string(),
        protocol_version: APP_SERVER_PROTOCOL_VERSION.to_string(),
        app_version: "0.1.0".to_string(),
        lifecycle: ClientProbeLifecycle::Running,
    };

    let value = serde_json::to_value(result).unwrap();

    assert_eq!(value["stateRootFingerprint"], json!("root-a"));
    assert_eq!(value["protocolVersion"], json!("1"));
    assert_eq!(value["appVersion"], json!("0.1.0"));
    assert_eq!(value["lifecycle"], json!("running"));
}

#[test]
fn initialize_params_use_typed_method_shape() {
    let params = InitializeParams {
        client_instance_id: "client-1".into(),
        shell: ShellDescriptor {
            kind: ShellKind::VscodeExtension,
            name: Some("OpenAIDE".to_string()),
            version: None,
        },
        requested_surface: RequestedSurface::Task {
            task_id: "task-1".into(),
        },
        capabilities: ClientCapabilities {
            protocol: vec![ClientProtocolCapability::StableClientRequestIds],
            shell: vec![ShellCapability::RevealFile],
        },
        workspace_roots: vec![ClientWorkspaceRoot {
            path: "/workspace/app".to_string(),
        }],
    };

    let value = serde_json::to_value(params).unwrap();

    assert_eq!(value["clientInstanceId"], json!("client-1"));
    assert_eq!(value["shell"]["kind"], json!("vscodeExtension"));
    assert_eq!(value["requestedSurface"]["kind"], json!("task"));
    assert_eq!(value["requestedSurface"]["taskId"], json!("task-1"));
    assert_eq!(
        value["capabilities"]["protocol"],
        json!(["stableClientRequestIds"])
    );
    assert_eq!(value["workspaceRoots"][0]["path"], json!("/workspace/app"));
}

#[test]
fn capabilities_changed_distinguishes_omitted_roots_from_an_empty_replacement() {
    let unchanged = serde_json::to_value(ClientCapabilitiesChangedParams::default()).unwrap();
    let cleared = serde_json::to_value(ClientCapabilitiesChangedParams {
        capabilities: None,
        workspace_roots: Some(Vec::new()),
    })
    .unwrap();

    assert!(unchanged.get("workspaceRoots").is_none());
    assert_eq!(cleared["workspaceRoots"], json!([]));
}
