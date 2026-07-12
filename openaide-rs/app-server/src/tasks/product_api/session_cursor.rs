use std::collections::HashSet;

/// Traverses opaque Agent cursors while enforcing forward progress.
///
/// Cursor values have no ordering semantics, but repeated values still prove a
/// pagination cycle. The caller owns page collection and filtering policy.
pub(super) struct OpaqueSessionCursor {
    current: Option<String>,
    seen: HashSet<String>,
}

impl OpaqueSessionCursor {
    pub(super) fn new(initial: Option<String>) -> Self {
        let seen = initial.iter().cloned().collect();
        Self {
            current: initial,
            seen,
        }
    }

    pub(super) fn current(&self) -> Option<String> {
        self.current.clone()
    }

    /// Returns the next unique cursor, or `None` when history is exhausted or cyclic.
    pub(super) fn advance(&mut self, next: Option<String>) -> Option<String> {
        let next = next?;
        if !self.seen.insert(next.clone()) {
            self.current = None;
            return None;
        }
        self.current = Some(next.clone());
        Some(next)
    }
}
