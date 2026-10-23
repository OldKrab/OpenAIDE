//! Retained Terminal output for clients that attach later (ADR-0071).

use std::collections::VecDeque;

/// About 10,000 lines of an ordinary shell session.
pub(crate) const REPLAY_MAX_LINES: usize = 10_000;
/// Bounds memory when lines are long or output has no newlines at all.
pub(crate) const REPLAY_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Keeps the newest output within both limits. Trimming cuts after a newline so a replay never
/// starts in the middle of a line or of the escape sequence that styles it; output without any
/// newline falls back to a byte cut.
pub(crate) struct Replay {
    bytes: VecDeque<u8>,
    lines: usize,
    max_lines: usize,
    max_bytes: usize,
}

impl Default for Replay {
    fn default() -> Self {
        Self::with_limits(REPLAY_MAX_LINES, REPLAY_MAX_BYTES)
    }
}

impl Replay {
    pub(crate) fn with_limits(max_lines: usize, max_bytes: usize) -> Self {
        Self {
            bytes: VecDeque::new(),
            lines: 0,
            max_lines,
            max_bytes,
        }
    }

    pub(crate) fn push(&mut self, output: &[u8]) {
        self.lines += output.iter().filter(|byte| **byte == b'\n').count();
        self.bytes.extend(output);
        while self.lines > self.max_lines || self.bytes.len() > self.max_bytes {
            match self.bytes.iter().position(|byte| *byte == b'\n') {
                Some(newline) => {
                    self.bytes.drain(..=newline);
                    self.lines -= 1;
                }
                None => {
                    let excess = self.bytes.len().saturating_sub(self.max_bytes);
                    self.bytes.drain(..excess);
                    break;
                }
            }
        }
    }

    pub(crate) fn contents(&self) -> Vec<u8> {
        self.bytes.iter().copied().collect()
    }
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
