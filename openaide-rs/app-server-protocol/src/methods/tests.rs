use super::*;

#[test]
fn client_initialize_uses_slash_method_namespace() {
    assert_eq!(ClientProbe::METHOD, "client/probe");
    assert_eq!(ClientInitialize::METHOD, "client/initialize");
    assert_eq!(
        ClientCapabilitiesChanged::METHOD,
        "client/capabilitiesChanged"
    );
}

#[test]
fn state_methods_use_state_namespace() {
    assert_eq!(StateSubscribe::METHOD, "state/subscribe");
    assert_eq!(StateUnsubscribe::METHOD, "state/unsubscribe");
}

#[test]
fn support_methods_use_support_namespace() {
    assert_eq!(
        SupportRecoverStuckSessions::METHOD,
        "support/recoverStuckSessions"
    );
}

#[test]
fn agent_methods_use_agent_namespace() {
    assert_eq!(AgentProbe::METHOD, "agent/probe");
    assert_eq!(AgentListSessions::METHOD, "agent/listSessions");
    assert_eq!(AgentCreateCustom::METHOD, "agent/createCustom");
    assert_eq!(
        AgentUpdateCustomMetadata::METHOD,
        "agent/updateCustomMetadata"
    );
    assert_eq!(AgentReplaceCustom::METHOD, "agent/replaceCustom");
    assert_eq!(AgentDeleteCustom::METHOD, "agent/deleteCustom");
    assert_eq!(AgentSetEnabled::METHOD, "agent/setEnabled");
}

#[test]
fn attachment_methods_use_attachment_namespace() {
    assert_eq!(AttachmentListRoots::METHOD, "attachment/listRoots");
    assert_eq!(AttachmentListDirectory::METHOD, "attachment/listDirectory");
    assert_eq!(
        AttachmentCreateFileReference::METHOD,
        "attachment/createFileReference"
    );
    assert_eq!(
        AttachmentCreatePastedImage::METHOD,
        "attachment/createPastedImage"
    );
    assert_eq!(
        AttachmentCreateEmbeddedCandidate::METHOD,
        "attachment/createEmbeddedCandidate"
    );
    assert_eq!(
        AttachmentConfirmEmbedded::METHOD,
        "attachment/confirmEmbedded"
    );
    assert_eq!(
        AttachmentRefreshHandles::METHOD,
        "attachment/refreshHandles"
    );
    assert_eq!(AttachmentRelease::METHOD, "attachment/release");
    assert_eq!(AttachmentReveal::METHOD, "attachment/reveal");
}

#[test]
fn task_methods_use_task_namespace() {
    assert_eq!(TaskCreate::METHOD, "task/create");
    assert_eq!(TaskAdoptNativeSession::METHOD, "task/adoptNativeSession");
    assert_eq!(TaskSend::METHOD, "task/send");
    assert_eq!(TaskSetConfigOption::METHOD, "task/setConfigOption");
    assert_eq!(TaskCancel::METHOD, "task/cancel");
    assert_eq!(TaskOpen::METHOD, "task/open");
    assert_eq!(TaskMarkRead::METHOD, "task/markRead");
    assert_eq!(TaskList::METHOD, "task/list");
    assert_eq!(TaskDiscard::METHOD, "task/discard");
    assert_eq!(TaskSetArchived::METHOD, "task/setArchived");
}
