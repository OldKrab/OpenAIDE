#[test]
fn task_queries_remain_read_only() {
    let text = std::fs::read_to_string("src/tasks/query.rs").unwrap();
    assert_no_patterns(
        "TaskQueries",
        &text,
        &[
            "commit_existing_task(",
            "create_task(",
            "TaskMutations",
            "crate::storage::Store",
            ".write_task(",
            ".append_message(",
            ".upsert_message_by_identity(",
            "append_normalized_to_store(",
            "upsert_normalized_to_store(",
            "RuntimeNotifier",
            "AgentGateway",
            "TurnRunner",
        ],
    );

    let read_store_text = std::fs::read_to_string("src/tasks/query_store.rs").unwrap();
    assert_no_patterns(
        "TaskReadStore",
        &read_store_text,
        &[
            ".write_task(",
            ".append_message(",
            ".upsert_message_by_identity(",
            "append_normalized_to_store(",
            "upsert_normalized_to_store(",
            ".finish_running_activities(",
            ".resolve_permission(",
            ".write_tool_artifact(",
        ],
    );
}

#[test]
fn task_commands_do_not_bypass_task_mutations() {
    let text = std::fs::read_to_string("src/tasks/task_commands.rs").unwrap();
    assert_no_patterns(
        "TaskCommands",
        &text,
        &[
            ".write_task(",
            ".append_message(",
            ".upsert_message_by_identity(",
            "append_normalized_to_store(",
            "upsert_normalized_to_store(",
            ".task_updated(",
            "RuntimeNotifier",
        ],
    );
    assert!(
        text.contains("commit_existing_task("),
        "TaskCommands should route durable mutations through TaskMutations"
    );
}

#[test]
fn task_service_delegates_public_agent_requests() {
    let text = std::fs::read_to_string("src/tasks/service.rs").unwrap();
    assert_no_patterns(
        "TaskService public Agent methods",
        &text,
        &[
            "AgentProbeRequest",
            "AgentAuthenticateRequest",
            "AgentListSessionsRequest",
        ],
    );
    assert!(
        text.contains("agent_service: AgentService"),
        "TaskService should compose AgentService for public Agent utility methods"
    );
}

fn assert_no_patterns(owner: &str, text: &str, patterns: &[&str]) {
    let offenders = patterns
        .iter()
        .filter(|pattern| text.contains(**pattern))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "{owner} boundary contains forbidden patterns: {}",
        offenders.join(", ")
    );
}
