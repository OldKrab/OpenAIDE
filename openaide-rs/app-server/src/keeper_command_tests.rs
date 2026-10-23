use openaide_app_server_protocol::client::{
    AttachedClientSummary, ClientReplacementImpactResult, ShellKind,
};

use super::*;

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

fn client(kind: ShellKind) -> AttachedClientSummary {
    AttachedClientSummary {
        kind,
        name: None,
        app_version: None,
        protocol_version: None,
        reconnecting: false,
    }
}

#[test]
fn reads_start_modes_and_refuses_anything_else() {
    assert_eq!(parse(&words("start")), Ok(Action::Start(Mode::Own)));
    assert_eq!(
        parse(&words("start --replace")),
        Ok(Action::Start(Mode::Replace))
    );
    assert_eq!(parse(&words("run --join")), Ok(Action::Run(Mode::Join)));
    assert_eq!(parse(&words("stop")), Ok(Action::Stop));
    assert!(parse(&words("start --replace --join")).is_err());
    assert!(parse(&words("stop now")).is_err());
    assert!(parse(&words("restart")).is_err());
}

#[test]
fn only_app_shells_count_as_attached() {
    let impact = ClientReplacementImpactResult {
        other_clients: vec![
            client(ShellKind::Desktop),
            client(ShellKind::Mobile),
            client(ShellKind::Terminal),
            client(ShellKind::Keeper),
        ],
        active_turns: 1,
        pending_task_requests: 2,
    };

    assert_eq!(
        observation_from(&impact),
        Observation {
            active_turns: 1,
            pending_task_requests: 2,
            app_shells: 2,
        }
    );
}

#[test]
fn a_failure_report_names_only_its_class() {
    let line = serde_json::to_string(&Report::failed("not_running")).unwrap();

    assert_eq!(line, r#"{"outcome":"failed","error":"not_running"}"#);
}
