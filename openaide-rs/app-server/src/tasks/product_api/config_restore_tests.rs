//! A Task keeps the option values the user chose across an Agent restoring its session.

use super::*;
use crate::storage::agent_config_preferences::AgentConfigPreference;

fn api_for(store: &Store, agent: Arc<RecordingAgent>) -> TaskProductApi {
    TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        agent,
        TaskUpdateNotifier::disabled(),
    )
    .unwrap()
}

fn chosen_model(value: &str) -> Vec<AgentConfigPreference> {
    vec![AgentConfigPreference {
        id: "model".to_string(),
        value: ConfigOptionCurrentValue::id(value),
    }]
}

#[test]
fn a_confirmed_change_is_remembered_for_the_task() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-chosen", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("session-live".to_string());
    record.config_options_catalog = Some(config_catalog("gpt-5"));
    store.write_task(&record).unwrap();
    let api = api_for(&store, Arc::new(RecordingAgent::default()));

    api.set_config_option_for_test(TaskSetConfigOptionParams {
        task_id: "task-chosen".into(),
        config_id: "model".into(),
        value: protocol_config_id("gpt-5.5"),
        client_mutation_id: "mutation-1".into(),
    })
    .unwrap();

    let stored = store.read_task("task-chosen").unwrap();
    assert_eq!(stored.config_mutation.chosen, chosen_model("gpt-5.5"));
}

/// The reported failure: an App Server restart resumes the session, and the Agent
/// answers with its defaults instead of the model the user picked.
#[test]
fn opening_a_task_whose_agent_restored_defaults_reapplies_the_chosen_values() {
    let diagnostic_logs = crate::logging::capture_test_logs();
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-restored", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("native-session".to_string());
    record.config_mutation.chosen = chosen_model("gpt-5.5");
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5")),
        ..Default::default()
    });
    let api = api_for(&store, agent.clone());

    api.open_for_test(TaskOpenParams {
        task_id: "task-restored".into(),
    })
    .unwrap();

    wait_until(|| {
        store
            .read_task("task-restored")
            .is_ok_and(|task| task_config_id(&task, "model") == Some("gpt-5.5"))
    });
    assert_eq!(
        agent.session_config_updates.lock().unwrap().as_slice(),
        [(
            "native-session".to_string(),
            "model".to_string(),
            "gpt-5.5".to_string()
        )]
    );
    let restore = diagnostic_logs
        .snapshot()
        .into_iter()
        .find(|entry| entry["event"] == "task_config_restore_completed")
        .expect("restore is logged");
    assert_eq!(restore["fields"]["outcome"], "settled");
    assert_eq!(restore["fields"]["task_id"], "task-restored");
}

#[test]
fn a_restored_session_that_kept_the_chosen_values_needs_no_change() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-kept", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("native-session".to_string());
    record.config_mutation.chosen = chosen_model("gpt-5.5");
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5.5")),
        ..Default::default()
    });
    let api = api_for(&store, agent.clone());

    api.open_for_test(TaskOpenParams {
        task_id: "task-kept".into(),
    })
    .unwrap();

    wait_until(|| {
        agent.resumes.load(Ordering::SeqCst) == 1
            && store
                .read_task("task-kept")
                .is_ok_and(|task| task.config_options_catalog.is_some())
    });
    assert!(agent.session_config_updates.lock().unwrap().is_empty());
}

#[test]
fn a_chosen_value_the_agent_no_longer_offers_is_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut record = task_record("task-retired", "/tmp/openaide-unit-workspace/app");
    record.agent_session_id = Some("native-session".to_string());
    record.config_mutation.chosen = chosen_model("retired-model");
    store.write_task(&record).unwrap();
    let agent = Arc::new(RecordingAgent {
        resume_config_catalog: Some(config_catalog("gpt-5")),
        ..Default::default()
    });
    let api = api_for(&store, agent.clone());

    api.open_for_test(TaskOpenParams {
        task_id: "task-retired".into(),
    })
    .unwrap();

    wait_until(|| {
        store
            .read_task("task-retired")
            .is_ok_and(|task| task_config_id(&task, "model") == Some("gpt-5"))
    });
    assert!(agent.session_config_updates.lock().unwrap().is_empty());
}
