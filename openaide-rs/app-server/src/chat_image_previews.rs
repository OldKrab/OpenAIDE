//! Chat image previews (ADR-0084): the copy of a stored Chat image that a Chat row
//! shows, so a Task with screenshots costs kilobytes per image on a slow link instead
//! of megabytes. The original stays the stored image; the full-size view loads it.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::image_preview::{self, PreviewBounds};

/// A Chat row is at most 320 CSS pixels wide and 240 tall, so 960 pixels stays sharp
/// on a 3x phone screen.
const BOUNDS: PreviewBounds = PreviewBounds {
    max_edge: 960,
    max_bytes: 256 * 1024,
    jpeg_quality: 80,
};

/// Bound on cached preview bytes: a few hundred screenshots at typical preview sizes.
const CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// An entry that holds no bytes still counts this much, which bounds the entry count.
const ENTRY_MIN_BYTES: usize = 1024;

/// What a Chat row should load for one stored image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatPreview {
    /// A smaller copy, encoded again.
    Reduced {
        media_type: &'static str,
        bytes: Arc<[u8]>,
    },
    /// The original bytes: already small, not smaller as a copy, or not decodable here.
    Original,
}

/// How one preview was produced, for the request's diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreviewOutcome {
    pub preview: ChatPreview,
    pub cache_hit: bool,
    /// Time spent waiting for the shared decode gate, zero on a cache hit.
    pub gate_wait_ms: u128,
    /// Why no smaller copy exists, when the reason is a failure rather than size.
    pub fallback_reason: Option<&'static str>,
}

/// Previews made since App Server started, keyed by image id. An id is the SHA-256 of
/// the image's bytes, so an entry never goes stale and serves every Task that shows
/// the same image; callers check Task access before asking.
#[derive(Clone, Default)]
pub(crate) struct ChatImagePreviews {
    cache: Arc<Mutex<Cache>>,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<String, (ChatPreview, Option<&'static str>)>,
    /// Least recently used first.
    order: VecDeque<String>,
    held: usize,
}

impl ChatImagePreviews {
    /// Returns the preview of `original`, making it under the shared decode gate on a
    /// miss. Runs outside the protocol lock: decoding a large screenshot takes long
    /// enough to stall every other client.
    pub(crate) fn preview(&self, image_id: &str, original: &[u8]) -> PreviewOutcome {
        if let Some(hit) = self.cached(image_id) {
            return hit;
        }
        let waiting_since = Instant::now();
        let Ok(_permit) = image_preview::GATE.lock() else {
            return PreviewOutcome {
                preview: ChatPreview::Original,
                cache_hit: false,
                gate_wait_ms: waiting_since.elapsed().as_millis(),
                fallback_reason: Some("gate_poisoned"),
            };
        };
        let gate_wait_ms = waiting_since.elapsed().as_millis();
        // Another request may have made it while this one waited.
        if let Some(hit) = self.cached(image_id) {
            return PreviewOutcome {
                gate_wait_ms,
                ..hit
            };
        }
        let (preview, fallback_reason) = make(original);
        self.remember(image_id, preview.clone(), fallback_reason);
        PreviewOutcome {
            preview,
            cache_hit: false,
            gate_wait_ms,
            fallback_reason,
        }
    }

    fn cached(&self, image_id: &str) -> Option<PreviewOutcome> {
        let mut cache = self.cache.lock().ok()?;
        let (preview, fallback_reason) = cache.entries.get(image_id).cloned()?;
        cache.touch(image_id);
        Some(PreviewOutcome {
            preview,
            cache_hit: true,
            gate_wait_ms: 0,
            fallback_reason,
        })
    }

    fn remember(
        &self,
        image_id: &str,
        preview: ChatPreview,
        fallback_reason: Option<&'static str>,
    ) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        cache.insert(image_id.to_string(), preview, fallback_reason);
    }

    #[cfg(test)]
    pub(crate) fn held_bytes(&self) -> usize {
        self.cache.lock().unwrap().held
    }
}

impl Cache {
    fn touch(&mut self, image_id: &str) {
        if let Some(position) = self.order.iter().position(|id| id == image_id) {
            if let Some(id) = self.order.remove(position) {
                self.order.push_back(id);
            }
        }
    }

    fn insert(&mut self, image_id: String, preview: ChatPreview, reason: Option<&'static str>) {
        if self.entries.contains_key(&image_id) {
            return;
        }
        let size = entry_bytes(&preview);
        if size > CACHE_MAX_BYTES {
            return;
        }
        self.held += size;
        self.entries.insert(image_id.clone(), (preview, reason));
        self.order.push_back(image_id);
        while self.held > CACHE_MAX_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((evicted, _)) = self.entries.remove(&oldest) {
                self.held -= entry_bytes(&evicted);
            }
        }
    }
}

fn entry_bytes(preview: &ChatPreview) -> usize {
    match preview {
        ChatPreview::Reduced { bytes, .. } => bytes.len().max(ENTRY_MIN_BYTES),
        ChatPreview::Original => ENTRY_MIN_BYTES,
    }
}

/// Never returns a copy that is not smaller than the original: a small or already
/// compressed image is cheaper to send as it is, with its animation and exact colors.
fn make(original: &[u8]) -> (ChatPreview, Option<&'static str>) {
    let Some((format, media_type)) = image_preview::sniff(original) else {
        return (ChatPreview::Original, Some("unknown_format"));
    };
    match image_preview::reduce(original.to_vec(), format, media_type, BOUNDS) {
        Ok(preview) if preview.reduced && preview.bytes.len() < original.len() => (
            ChatPreview::Reduced {
                media_type: preview.media_type,
                bytes: preview.bytes.into(),
            },
            None,
        ),
        Ok(_) => (ChatPreview::Original, None),
        Err(image_preview::Unsupported) => (ChatPreview::Original, Some("undecodable")),
    }
}

#[cfg(test)]
#[path = "chat_image_previews_tests.rs"]
mod tests;
