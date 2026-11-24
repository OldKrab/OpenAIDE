//! A request's stream, and a connection a switch reached, as Kotlin holds them.

use iroh::endpoint::{Connection, RecvStream, SendStream};
use tokio::sync::Mutex;

use crate::on_runtime;

/// One side of the stream ended or failed. The connection's own failure, if any,
/// is learned by the next open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum StreamError {
    #[error("failed")]
    Failed,
}

/// A connection reached for a switch, held until the switch adopts or abandons it.
#[derive(uniffi::Object)]
pub struct Reached(pub(crate) Connection);

/// One request and its response, as bytes in each direction. The two halves are
/// used from separate threads: the request body is sent while the response is read.
#[derive(uniffi::Object)]
pub struct Stream {
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
}

impl Stream {
    pub(crate) fn new(send: SendStream, recv: RecvStream) -> Self {
        Self {
            send: Mutex::new(send),
            recv: Mutex::new(recv),
        }
    }
}

#[uniffi::export]
impl Stream {
    pub async fn write_all(self: std::sync::Arc<Self>, bytes: Vec<u8>) -> Result<(), StreamError> {
        on_runtime(async move {
            let mut send = self.send.lock().await;
            send.write_all(&bytes)
                .await
                .map_err(|_| StreamError::Failed)
        })
        .await
    }

    /// Ends the request; the response may still arrive.
    pub async fn finish(self: std::sync::Arc<Self>) -> Result<(), StreamError> {
        on_runtime(async move {
            let mut send = self.send.lock().await;
            send.finish().map_err(|_| StreamError::Failed)
        })
        .await
    }

    /// Up to `max` bytes of the response; empty once it has ended.
    pub async fn read(self: std::sync::Arc<Self>, max: u32) -> Result<Vec<u8>, StreamError> {
        on_runtime(async move {
            let mut recv = self.recv.lock().await;
            let mut buffer = vec![0u8; max as usize];
            match recv.read(&mut buffer).await {
                Ok(Some(count)) => {
                    buffer.truncate(count);
                    Ok(buffer)
                }
                Ok(None) => Ok(Vec::new()),
                Err(_) => Err(StreamError::Failed),
            }
        })
        .await
    }

    /// The whole response, failing when it is longer than `limit` bytes.
    pub async fn read_to_end(
        self: std::sync::Arc<Self>,
        limit: u32,
    ) -> Result<Vec<u8>, StreamError> {
        on_runtime(async move {
            let mut recv = self.recv.lock().await;
            recv.read_to_end(limit as usize)
                .await
                .map_err(|_| StreamError::Failed)
        })
        .await
    }
}
