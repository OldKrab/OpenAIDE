//! Methods that open, attach to, and close Terminals.

use super::{ProtocolMethod, TERMINALS_ATTACH, TERMINALS_CLOSE, TERMINALS_OPEN};
use crate::terminals::{
    TerminalsAttachParams, TerminalsAttachResult, TerminalsCloseParams, TerminalsCloseResult,
    TerminalsOpenParams, TerminalsOpenResult,
};

protocol_method!(
    TerminalsOpen,
    TERMINALS_OPEN,
    TerminalsOpenParams,
    TerminalsOpenResult
);
protocol_method!(
    TerminalsAttach,
    TERMINALS_ATTACH,
    TerminalsAttachParams,
    TerminalsAttachResult
);
protocol_method!(
    TerminalsClose,
    TERMINALS_CLOSE,
    TerminalsCloseParams,
    TerminalsCloseResult
);
