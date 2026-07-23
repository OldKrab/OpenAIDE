use crate::protocol::model::{
    ConfigOption, ConfigOptionCategory, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionValue, ConfigOptionsCatalog, ConfigOptionsStatus,
};

use super::Store;

#[test]
fn agent_config_preferences_round_trip_without_crossing_agent_identity() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    assert!(store
        .write_agent_config_preferences(&catalog("codex", "agent-full-access"))
        .expect("persist Codex preferences"));
    assert!(store
        .write_agent_config_preferences(&catalog("opencode", "plan"))
        .expect("persist OpenCode preferences"));
    assert!(!store
        .write_agent_config_preferences(&catalog("codex", "agent-full-access"))
        .expect("skip unchanged Codex preferences"));
    drop(store);

    let reopened = Store::open(root.path().to_path_buf()).expect("reopen store");
    assert_eq!(
        reopened
            .read_agent_config_preferences("codex")
            .expect("read Codex preferences")
            .options[0]
            .value,
        ConfigOptionCurrentValue::id("agent-full-access")
    );
    assert_eq!(
        reopened
            .read_agent_config_preferences("opencode")
            .expect("read OpenCode preferences")
            .options[0]
            .value,
        ConfigOptionCurrentValue::id("plan")
    );
}

fn catalog(agent_id: &str, current_mode: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: agent_id.to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "mode".to_string(),
            label: "Mode".to_string(),
            description: None,
            category: Some(ConfigOptionCategory::Mode),
            kind: ConfigOptionKind::Select,
            current_value: ConfigOptionCurrentValue::id(current_mode),
            values: vec![ConfigOptionValue {
                id: current_mode.to_string(),
                label: current_mode.to_string(),
                description: None,
                group_id: None,
                group_label: None,
            }],
        }],
    }
}
