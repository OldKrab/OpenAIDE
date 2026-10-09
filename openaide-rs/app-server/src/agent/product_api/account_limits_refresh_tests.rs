use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use openaide_app_server_protocol::snapshot::AgentAccountLimitWindowKind;

use super::start;
use crate::agent::events::{
    AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
};
use crate::agent::gateway::AgentGateway;
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::{AgentEventSink, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart};
use crate::protocol::errors::RuntimeError;

const CLAUDE: &str = "claude-code";

/// Answers every read with `answer` and counts how often it was asked.
struct LimitsAgent {
    reads: Arc<AtomicUsize>,
    answer: Result<Option<AgentAccountLimitsChange>, RuntimeError>,
}

impl AgentRuntime for LimitsAgent {
    fn read_account_limits(
        &self,
        _agent_id: &str,
    ) -> Result<Option<AgentAccountLimitsChange>, RuntimeError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }

    fn start_session(&self, _request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        unreachable!("a limits refresh must not start a session")
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        unreachable!("a limits refresh must not prompt")
    }
}

fn five_hour(used_percent: u8) -> AgentAccountLimitsChange {
    AgentAccountLimitsChange::Usage {
        plan_label: Some("Pro".to_string()),
        windows: vec![AgentAccountLimitUsage {
            window: AgentAccountLimitWindowId {
                kind: AgentAccountLimitWindowKind::FiveHour,
                model_label: None,
            },
            used_percent,
            resets_at_ms: None,
        }],
    }
}

fn gateway(
    answer: Result<Option<AgentAccountLimitsChange>, RuntimeError>,
) -> (AgentGateway, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let agent = LimitsAgent {
        reads: reads.clone(),
        answer,
    };
    (AgentGateway::new(Arc::new(agent)), reads)
}

// timing: data — the minimum gap between reads, compared against elapsed time and never waited on.
const INTERVAL: Duration = Duration::from_secs(60);

#[test]
fn a_refresh_reads_in_the_background_and_publishes_the_reading() {
    let (gateway, reads) = gateway(Ok(Some(five_hour(39))));
    let (statuses, updates) = AgentStatusCache::channel();

    let read = start(&gateway, &statuses, CLAUDE, INTERVAL).expect("a read starts");
    read.join().expect("read thread");

    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        statuses.account_limits(CLAUDE).expect("limits").windows[0].used_percent,
        39
    );
    assert!(updates.try_recv().is_ok(), "clients learn the new reading");
    assert!(
        start(&gateway, &statuses, CLAUDE, INTERVAL).is_none(),
        "a fresh reading answers the next request without a read"
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn a_refresh_keeps_the_shown_limits_when_the_agent_has_none_or_fails() {
    for answer in [
        Ok(None),
        Err(RuntimeError::NotReady("agent process ended".to_string())),
    ] {
        let (gateway, _reads) = gateway(answer);
        let statuses = AgentStatusCache::default();
        statuses.record_account_limits(CLAUDE, five_hour(12));

        start(&gateway, &statuses, CLAUDE, Duration::ZERO)
            .expect("a read starts")
            .join()
            .expect("read thread");

        assert_eq!(
            statuses.account_limits(CLAUDE).expect("limits").windows[0].used_percent,
            12
        );
        assert!(
            start(&gateway, &statuses, CLAUDE, INTERVAL).is_none(),
            "a failed attempt is not retried at once"
        );
    }
}

#[test]
fn an_agent_without_the_read_is_asked_only_once() {
    let (gateway, reads) = gateway(Err(RuntimeError::MethodNotFound(
        "_claude/accountLimits/read".to_string(),
    )));
    let statuses = AgentStatusCache::default();

    start(&gateway, &statuses, CLAUDE, Duration::ZERO)
        .expect("a read starts")
        .join()
        .expect("read thread");

    assert!(start(&gateway, &statuses, CLAUDE, Duration::ZERO).is_none());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn no_read_starts_for_another_agent_or_one_that_is_not_signed_in() {
    let (gateway, reads) = gateway(Ok(Some(five_hour(39))));
    let statuses = AgentStatusCache::default();
    assert!(start(&gateway, &statuses, "codex", Duration::ZERO).is_none());

    statuses.record_session_error(
        CLAUDE,
        &RuntimeError::AuthRequired("sign in".to_string()),
        None,
    );
    assert!(start(&gateway, &statuses, CLAUDE, Duration::ZERO).is_none());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
