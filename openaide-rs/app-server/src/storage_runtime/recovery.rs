#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryClassification {
    CleanOpen,
    UncleanPreviousShutdown,
}
