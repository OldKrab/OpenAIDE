use openaide_app_server_protocol::ids::EventCursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotReadToken {
    cursor: EventCursor,
}

impl SnapshotReadToken {
    pub fn cursor(&self) -> &EventCursor {
        &self.cursor
    }
}

#[derive(Debug, Clone)]
pub struct CursorSequencer {
    sequence: u64,
}

impl CursorSequencer {
    pub fn new() -> Self {
        Self { sequence: 0 }
    }

    pub fn read_token(&self) -> SnapshotReadToken {
        SnapshotReadToken {
            cursor: EventCursor::from(format!("cursor-{}", self.sequence)),
        }
    }

    pub fn advance(&mut self) -> (EventCursor, EventCursor) {
        let previous = EventCursor::from(format!("cursor-{}", self.sequence));
        self.sequence += 1;
        let cursor = EventCursor::from(format!("cursor-{}", self.sequence));
        (previous, cursor)
    }
}

impl Default for CursorSequencer {
    fn default() -> Self {
        Self::new()
    }
}
