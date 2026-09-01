use super::*;

#[test]
fn url_elicitation_opens_the_destination_through_the_app_shell() {
    let (host_bridge, requests) = HostBridge::channel();
    let response_bridge = host_bridge.clone();
    let host = std::thread::spawn(move || {
        let request = requests.recv().expect("open external request");
        assert_eq!(request.method, "shell/openExternal");
        assert_eq!(
            request.params.as_ref().expect("open external params")["url"],
            "https://auth.openai.com/device"
        );
        assert!(response_bridge.try_handle_response(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": { "opened": true }
        })));
    });
    let session_event_sinks: AcpSessionEventSinkMap = Arc::default();
    let handlers = AcpHostCapabilityHandlers::new(AcpHostCapabilityContext {
        host_bridge: host_bridge.clone(),
        trace: None,
        current_prompts: Arc::default(),
        terminal_registry: AcpHostTerminalRegistry::new(host_bridge),
        session_event_sinks: session_event_sinks.clone(),
        session_traces: Arc::default(),
        elicitation_cancellations: Arc::default(),
        native_subagents: AcpNativeSubagentRouter::new("test-agent", session_event_sinks),
    });
    let request = serde_json::from_value(serde_json::json!({
        "requestId": 7,
        "mode": "url",
        "message": "Sign in to ChatGPT and enter this code: ABCD-EFGH",
        "elicitationId": "login-1",
        "url": "https://auth.openai.com/device"
    }))
    .expect("url elicitation request");

    let response = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(handlers.create_elicitation_inner(WireRequestId::Integer(8), request))
        .expect("url elicitation response");

    assert!(matches!(response, ElicitationCreateResponse::Accept { .. }));
    host.join().expect("open external host");
}
