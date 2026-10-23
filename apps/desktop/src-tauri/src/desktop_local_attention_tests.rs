use std::cell::RefCell;

use super::*;

fn this_computer() -> HoldTarget {
    HoldTarget {
        endpoint_url: "http://127.0.0.1:1/probe".to_string(),
        auth_token: "a".repeat(64),
    }
}

/// A fake App Server: records the methods and params asked, answers from a script.
struct Server {
    asked: RefCell<Vec<(String, Value)>>,
    answers: RefCell<Vec<Result<Value, HoldError>>>,
}

impl Server {
    fn new(answers: Vec<Result<Value, HoldError>>) -> Self {
        Self {
            asked: RefCell::new(Vec::new()),
            answers: RefCell::new(answers.into_iter().rev().collect()),
        }
    }

    fn post(
        &self,
    ) -> impl FnMut(&HoldTarget, &str, &str, &str, Value, Duration) -> Result<Value, HoldError> + '_
    {
        move |_, _, _, method, params, _| {
            self.asked.borrow_mut().push((method.to_string(), params));
            self.answers.borrow_mut().pop().expect("no scripted answer")
        }
    }

    fn methods(&self) -> Vec<String> {
        self.asked
            .borrow()
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }
}

fn attention(cursor: &str, tasks: Value) -> Result<Value, HoldError> {
    Ok(json!({ "cursor": cursor, "tasks": tasks }))
}

#[test]
fn watches_nothing_while_this_computer_is_in_use() {
    let server = Server::new(vec![]);
    let mut watch = Watch::new();
    assert_eq!(watch.step(None, &mut server.post()), Step::Idle);
    assert!(server.methods().is_empty());
}

#[test]
fn joins_as_a_shell_hold_and_hands_over_the_first_list_then_only_changes() {
    let server = Server::new(vec![
        Ok(json!({})),
        attention("c1", json!([{ "task_id": "t1" }])),
        attention("c1", json!([{ "task_id": "t1" }])),
        attention("c2", json!([])),
    ]);
    let mut watch = Watch::new();
    let mut post = server.post();
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Changed(json!([{ "task_id": "t1" }]))
    );
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Unchanged
    );
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Changed(json!([]))
    );
    drop(post);
    let asked = server.asked.borrow();
    assert_eq!(asked[0].0, "client/initialize");
    assert_eq!(asked[0].1["role"], "shellHold");
    assert_eq!(asked[1].1, json!({}));
    assert_eq!(asked[2].1, json!({ "cursor": "c1" }));
    assert_eq!(asked[3].1, json!({ "cursor": "c1" }));
}

#[test]
fn starts_over_when_the_app_server_forgot_it_and_backs_off_on_failures() {
    let server = Server::new(vec![
        Ok(json!({})),
        Err(HoldError::Refused),
        Err(HoldError::Unreachable),
        Ok(json!({})),
        attention("c1", json!([])),
    ]);
    let mut watch = Watch::new();
    let mut post = server.post();
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Retry(Duration::from_secs(1))
    );
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Retry(Duration::from_secs(2))
    );
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Changed(json!([]))
    );
    drop(post);
    assert_eq!(
        server.methods(),
        [
            "client/initialize",
            "taskAttention/wait",
            "client/initialize",
            "client/initialize",
            "taskAttention/wait"
        ]
    );
}

#[test]
fn starts_over_after_this_computer_is_used_again() {
    let server = Server::new(vec![
        Ok(json!({})),
        attention("c1", json!([])),
        Ok(json!({})),
        attention("c1", json!([])),
    ]);
    let mut watch = Watch::new();
    let mut post = server.post();
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Changed(json!([]))
    );
    assert_eq!(watch.step(None, &mut post), Step::Idle);
    // A new watch hands over its first list again, so a reloaded window gets a baseline.
    assert_eq!(
        watch.step(Some(this_computer()), &mut post),
        Step::Changed(json!([]))
    );
}
