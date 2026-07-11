use std::future::Future;
use std::thread;

use crate::protocol::errors::RuntimeError;

pub(super) fn block_on_new_runtime<T>(future: impl Future<Output = T>) -> Result<T, RuntimeError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
    Ok(runtime.block_on(future))
}

pub(super) fn close_in_parallel(close_tasks: Vec<Box<dyn FnOnce() + Send + 'static>>) {
    let handles = close_tasks
        .into_iter()
        .map(thread::spawn)
        .collect::<Vec<_>>();
    for handle in handles {
        let _ = handle.join();
    }
}
