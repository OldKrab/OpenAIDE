use std::time::Duration;

use super::TurnCancellation;

#[test]
fn cancellation_wait_wakes_without_a_polling_interval() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let cancellation = TurnCancellation::new();
    let signal = cancellation.clone();

    runtime.block_on(async move {
        let waiting = tokio::spawn(async move { cancellation.cancelled().await });
        tokio::task::yield_now().await;
        signal.cancel();
        tokio::time::timeout(Duration::from_millis(20), waiting)
            .await
            .expect("cancellation wait should be notified directly")
            .unwrap();
    });
}
