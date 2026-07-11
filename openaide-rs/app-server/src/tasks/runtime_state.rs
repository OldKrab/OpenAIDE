#[derive(Default)]
pub struct RuntimeState {
    revision: u64,
}

impl RuntimeState {
    pub fn with_revision(revision: u64) -> Self {
        Self { revision }
    }

    pub fn current_revision(&self) -> u64 {
        self.revision
    }

    pub fn next_revision_candidate(&self) -> u64 {
        self.revision + 1
    }

    pub fn commit_revision(&mut self, revision: u64) {
        assert!(
            revision > self.revision,
            "committed revision must advance runtime state"
        );
        self.revision = revision;
    }

    pub fn next_revision(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }
}
