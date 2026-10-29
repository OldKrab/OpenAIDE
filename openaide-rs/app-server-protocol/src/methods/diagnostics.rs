//! Methods that report on the App Server itself and record what clients report to it.

use super::{
    ProtocolMethod, DIAGNOSTICS_CREATE_SUPPORT_EXPORT, DIAGNOSTICS_GET_RUNTIME,
    DIAGNOSTICS_LIST_SUPPORT_EXPORT, DIAGNOSTICS_RECORD_CLIENT_EVENTS,
    SUPPORT_RECOVER_STUCK_SESSIONS,
};
use crate::diagnostics::{
    ClientDiagnosticEventsParams, ClientDiagnosticEventsResult, RuntimeDiagnosticsParams,
    RuntimeDiagnosticsResult, SupportExportCreateParams, SupportExportCreateResult,
    SupportExportListParams, SupportExportListResult,
};
use crate::support::{SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult};

protocol_method!(
    DiagnosticsGetRuntime,
    DIAGNOSTICS_GET_RUNTIME,
    RuntimeDiagnosticsParams,
    RuntimeDiagnosticsResult
);
protocol_method!(
    DiagnosticsRecordClientEvents,
    DIAGNOSTICS_RECORD_CLIENT_EVENTS,
    ClientDiagnosticEventsParams,
    ClientDiagnosticEventsResult
);
protocol_method!(
    DiagnosticsListSupportExport,
    DIAGNOSTICS_LIST_SUPPORT_EXPORT,
    SupportExportListParams,
    SupportExportListResult
);
protocol_method!(
    DiagnosticsCreateSupportExport,
    DIAGNOSTICS_CREATE_SUPPORT_EXPORT,
    SupportExportCreateParams,
    SupportExportCreateResult
);
protocol_method!(
    SupportRecoverStuckSessions,
    SUPPORT_RECOVER_STUCK_SESSIONS,
    SupportRecoverStuckSessionsParams,
    SupportRecoverStuckSessionsResult
);
