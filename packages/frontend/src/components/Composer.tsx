import { ArrowUp, CircleStop, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AgentCommandsCatalog, AgentSlashCommand, ComposerSubmitShortcut, ConfigOptionCurrentValue, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { agentOptions, type AgentOption, type ComposerAttachment, type ComposerSelection } from "../state/composerOptions";
import {
  ComposerAttachments,
  type ComposerFileUpload,
} from "./ComposerAttachments";
import { ComposerControls, type ComposerMenu } from "./ComposerMenus";
import { ComposerEditor, type ComposerEditorHandle } from "./ComposerEditor";
import {
  composerErrorMessage,
  hasComposerContent,
  pastedImageFiles,
} from "./composerDraftPolicy";
import { composerCanSubmit, type ComposerAvailability } from "./composerAvailability";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import { IconButton } from "./ComposerPrimitives";
import { shouldInsertComposerNewline, shouldSubmitComposerKey } from "./composerKeymap";
import {
  commandCatalogKey,
  SlashCommandPicker,
  slashPickerForCatalog,
  replaceCommandToken,
  type SlashPickerState,
} from "./ComposerSlashCommands";
import { attachEveryImage } from "./imageAttachmentBatch";
import { useComposerAutoFocus } from "./useComposerAutoFocus";
import { useComposerKeyboardFocus } from "./useComposerKeyboardFocus";
import { usesMobileComposerBehavior } from "./mobileComposerBehavior";
import {
  FileMentionPicker,
  fileMentionTokenAtCursor,
  replaceFileMention,
  useFileMentionPicker,
  type FileMentionToken,
} from "./ComposerFileMentions";

export { shouldInsertComposerNewline, shouldSubmitComposerKey } from "./composerKeymap";

type ComposerProps = {
  agentLocked?: boolean;
  attachments: ComposerAttachment[];
  autoFocus?: boolean;
  availability: ComposerAvailability;
  commandCatalog?: AgentCommandsCatalog;
  configLocked?: boolean;
  configOptions?: ConfigOptionsCatalog;
  error?: string;
  fileBrowser?: TaskFileBrowserCallbacks;
  focusRequestKey?: number | string;
  imageAttachmentsAllowed?: boolean;
  agents?: AgentOption[];
  onCancel?: () => void;
  onChange: (prompt: string) => void;
  onUnsupportedImageAttachment?: (message?: string) => void;
  onRevealAttachment?: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onSelectAgent?: (agentId: string) => void;
  onSelectConfigOption?: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  onSubmit: (prompt: string) => void;
  prompt: string;
  selection: ComposerSelection;
  submitShortcut: ComposerSubmitShortcut;
  submissionSettlementKey?: number | string;
  showAgentSelector?: boolean;
  showIsolationSelector?: boolean;
};

export function Composer({
  agentLocked = false,
  attachments,
  autoFocus = false,
  availability,
  commandCatalog,
  configLocked = false,
  configOptions,
  error,
  fileBrowser,
  focusRequestKey,
  imageAttachmentsAllowed = true,
  agents = agentOptions,
  onCancel,
  onChange,
  onUnsupportedImageAttachment,
  onRevealAttachment,
  onRemoveAttachment,
  onSelectAgent,
  onSelectConfigOption,
  onSelectIsolation,
  onSubmit,
  prompt,
  selection,
  submitShortcut,
  submissionSettlementKey,
  showAgentSelector,
  showIsolationSelector,
}: ComposerProps) {
  const disabled = !availability.canEdit;
  const [openMenu, setOpenMenu] = useState<ComposerMenu | undefined>();
  const [slashPicker, setSlashPicker] = useState<SlashPickerState | undefined>();
  const [fileMentionToken, setFileMentionToken] = useState<FileMentionToken | undefined>();
  const [editorText, setEditorText] = useState(prompt);
  const [editorRenderRevision, setEditorRenderRevision] = useState(0);
  const [fileUploads, setFileUploads] = useState<ComposerFileUpload[]>([]);
  const { keyboardFocus, onKeyboardNavigation, onPointerInteraction } = useComposerKeyboardFocus();
  const editorRef = useRef<ComposerEditorHandle | null>(null);
  const configMutationSequenceRef = useRef(0);
  const fileDropHandlerRef = useRef<((files: File[]) => void) | undefined>(undefined);
  const draftRef = useRef(prompt);
  const lastPromptRef = useRef(prompt);
  const submittedDraftRef = useRef<string | undefined>(undefined);
  const commandCatalogRevision = commandCatalogKey(commandCatalog);
  const [optimisticConfigChange, setOptimisticConfigChange] = useState<NonNullable<ConfigOptionsCatalog["pending_change"]>>();
  const presentedConfigChange = configOptions?.pending_change ?? optimisticConfigChange;
  const presentedConfigOptions = configOptions && presentedConfigChange
    ? { ...configOptions, pending_change: presentedConfigChange }
    : configOptions;
  const configMutationId = presentedConfigChange?.mutation_id;
  const [showSlowConfigUpdate, setShowSlowConfigUpdate] = useState(false);
  const [filePicker, setFilePicker] = useFileMentionPicker(fileBrowser, fileMentionToken);
  const lastCommandCatalogKey = useRef(commandCatalogRevision);
  const lastSubmissionSettlementKey = useRef(submissionSettlementKey);
  const hasDraftContent = hasComposerContent(editorText, attachments.length);
  const uploadPending = fileUploads.some((upload) => upload.state !== "error");
  const canSubmit = composerCanSubmit(availability, editorText, attachments.length) && !uploadPending;

  useComposerAutoFocus({ autoFocus, disabled, editorRef, focusRequestKey });

  useEffect(() => {
    setShowSlowConfigUpdate(false);
    if (!configMutationId) return undefined;
    const timer = globalThis.setTimeout(() => setShowSlowConfigUpdate(true), 5_000);
    return () => globalThis.clearTimeout(timer);
  }, [configMutationId]);

  useEffect(() => {
    setOptimisticConfigChange((current) => {
      if (!current) return current;
      // Once App Server projects the mutation, its snapshot owns presentation.
      if (configOptions?.pending_change) return undefined;
      const option = configOptions?.options.find((candidate) => candidate.id === current.option_id);
      if (!option || configValueEquals(option.current_value, current.requested_value)) return undefined;
      return current;
    });
  }, [configOptions]);

  useEffect(() => {
    if (error) setOptimisticConfigChange(undefined);
  }, [error]);

  useEffect(() => {
    if (!openMenu || typeof document === "undefined") return undefined;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as { closest?: (selector: string) => Element | null } | null;
      // Only the active menu anchor and its popover are inside the dismissal
      // boundary. The rest of the Composer is a click-away surface.
      if (target?.closest?.(".composer-menu-anchor")) return;
      setOpenMenu(undefined);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [openMenu]);

  const selectConfigOption = (configId: string, value: ConfigOptionCurrentValue) => {
    if (!onSelectConfigOption) return;
    configMutationSequenceRef.current += 1;
    // Frontend owns this immediate feedback only until an authoritative snapshot arrives.
    setOptimisticConfigChange({
      mutation_id: `local:${configId}:${configMutationSequenceRef.current}`,
      option_id: configId,
      requested_value: value,
    });
    onSelectConfigOption(configId, value);
  };

  useEffect(() => {
    if (!openMenu || typeof window === "undefined") return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpenMenu(undefined);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openMenu]);

  useEffect(() => {
    if (!configLocked) return;
    // A connection reset can lock configuration while its menu is open. Close
    // that stale affordance before it can issue a mutation without a baseline.
    setOpenMenu((current) =>
      current === "options" || current?.startsWith("config:") ? undefined : current);
  }, [configLocked]);

  useEffect(() => {
    if (!disabled) return;
    // Sending freezes the full composer input, including any popover or slash
    // command flow that could otherwise complete after the request begins.
    setOpenMenu(undefined);
    setSlashPicker(undefined);
    setFileMentionToken(undefined);
  }, [disabled]);

  useEffect(() => {
    const settlementChanged = submissionSettlementKey !== lastSubmissionSettlementKey.current;
    lastSubmissionSettlementKey.current = submissionSettlementKey;
    const promptChanged = prompt !== lastPromptRef.current;
    lastPromptRef.current = prompt;

    // The submitted text remains visible while task/send is pending. It is cleared
    // only when the task-scoped draft is authoritatively reset after acceptance.
    const acceptedWithoutIntermediateRender = settlementChanged
      && submittedDraftRef.current !== undefined
      && prompt === "";
    if (promptChanged || acceptedWithoutIntermediateRender) {
      draftRef.current = prompt;
      renderEditorText(prompt);
    }
    if (!availability.submitting && (prompt === "" || error)) submittedDraftRef.current = undefined;
  }, [availability.submitting, error, prompt, submissionSettlementKey]);

  useEffect(() => {
    const catalogChanged = commandCatalogRevision !== lastCommandCatalogKey.current;
    lastCommandCatalogKey.current = commandCatalogRevision;
    if (commandCatalog?.status !== "ready") {
      setSlashPicker(undefined);
      return;
    }
    if (!catalogChanged) return;
    const draft = draftRef.current;
    const selection = editorRef.current?.selectionRange();
    const picker = slashPickerForCatalog(commandCatalog, draft, selection?.start ?? draft.length);
    setSlashPicker(picker);
    if (picker) setOpenMenu(undefined);
  }, [commandCatalogRevision, commandCatalog]);

  const toggleMenu = (menu: ComposerMenu) => {
    if (disabled) return;
    setOpenMenu((current) => (current === menu ? undefined : menu));
  };

  const selectAndClose = (select: () => void) => {
    select();
    setOpenMenu(undefined);
  };

  const renderEditorText = (value: string) => {
    setEditorText(value);
    setEditorRenderRevision((revision) => revision + 1);
  };

  const syncDraft = (value: string, options: { renderEditor?: boolean } = {}) => {
    draftRef.current = value;
    if (options.renderEditor) renderEditorText(value);
    else setEditorText(value);
  };

  const updateSlashPicker = (value: string, cursor: number) => {
    const picker = slashPickerForCatalog(commandCatalog, value, cursor);
    setSlashPicker(picker);
    if (picker) setOpenMenu(undefined);
  };

  const updateCompletionPickers = (value: string, cursor: number) => {
    updateSlashPicker(value, cursor);
    const token = fileMentionTokenAtCursor(value, cursor);
    setFileMentionToken(token);
    if (token) {
      setSlashPicker(undefined);
      setOpenMenu(undefined);
    }
  };

  const showStopAction = Boolean(onCancel && (!hasDraftContent || !canSubmit));
  const showSendAction = !onCancel || (hasDraftContent && canSubmit);

  const submitDraft = () => {
    const draft = draftRef.current;
    if (!canSubmit) return;
    submittedDraftRef.current = draft;
    onSubmit(draft);
  };

  const selectSlashCommand = (command: AgentSlashCommand) => {
    const picker = slashPicker;
    if (!picker) return;
    const next = replaceCommandToken(draftRef.current, picker.token, command);
    syncDraft(next.text, { renderEditor: true });
    onChange(next.text);
    setSlashPicker(undefined);
    queueEditorSelection(next.cursor);
  };

  const selectFileMention = (path: string) => {
    if (!filePicker) return;
    const next = replaceFileMention(draftRef.current, filePicker.token, path);
    syncDraft(next.text, { renderEditor: true });
    onChange(next.text);
    setFileMentionToken(undefined);
    queueEditorSelection(next.cursor);
  };

  const insertEditorText = (text: string) => {
    const selection = editorRef.current?.selectionRange() ?? {
      start: draftRef.current.length,
      end: draftRef.current.length,
    };
    const draft = draftRef.current;
    const nextText = `${draft.slice(0, selection.start)}${text}${draft.slice(selection.end)}`;
    const cursor = selection.start + text.length;
    syncDraft(nextText, { renderEditor: true });
    onChange(nextText);
    queueEditorSelection(cursor);
  };

  const queueEditorSelection = (cursor: number) => {
    const restore = () => {
      editorRef.current?.setSelectionRange(cursor, cursor);
      editorRef.current?.focus();
    };
    if (typeof queueMicrotask === "function") {
      queueMicrotask(restore);
    } else {
      setTimeout(restore, 0);
    }
  };

  return (
    <section
      className="composer"
      aria-label="Message composer"
      data-keyboard-focus={keyboardFocus ? "true" : undefined}
      onKeyDownCapture={onKeyboardNavigation}
      onPointerDownCapture={onPointerInteraction}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          setOpenMenu(undefined);
          setSlashPicker(undefined);
          setFileMentionToken(undefined);
        }
      }}
    >
      <ComposerAttachments
        attachments={attachments}
        disabled={disabled}
        onRemoveAttachment={onRemoveAttachment}
        onRevealAttachment={onRevealAttachment}
        uploads={fileUploads}
      />
      <ComposerEditor
        ariaLabel="Message"
        commandCatalog={commandCatalog}
        disabled={disabled}
        onInputText={(value, cursor) => {
          syncDraft(value);
          onChange(value);
          updateCompletionPickers(value, cursor);
        }}
        onDrop={(event) => {
          if (disabled) return;
          const dropped = Array.from(event.dataTransfer.files ?? []);
          if (dropped.length === 0) return;
          event.preventDefault();
          const images = dropped.filter((file) => file.type.startsWith("image/"));
          const files = dropped.filter((file) => !file.type.startsWith("image/"));
          if (images.length > 0) {
            if (!imageAttachmentsAllowed) {
              onUnsupportedImageAttachment?.("This Agent does not accept images.");
            } else if (!fileBrowser?.attachImage) {
              onUnsupportedImageAttachment?.("Images can be attached after the Task is open.");
            } else {
              const draft = { prompt: draftRef.current, context: attachments };
              void attachEveryImage(images, (image) => fileBrowser.attachImage(image, draft)).catch((error: unknown) => {
                onUnsupportedImageAttachment?.(composerErrorMessage(error, "Unable to attach image."));
              });
            }
          }
          if (files.length > 0) {
            if (fileDropHandlerRef.current) fileDropHandlerRef.current(files);
            else onUnsupportedImageAttachment?.("Files can be attached after the Task is open.");
          }
        }}
        onPointerDown={() => {
          setOpenMenu(undefined);
          setSlashPicker(undefined);
          setFileMentionToken(undefined);
        }}
        onPaste={(event) => {
          if (disabled) return;
          const images = pastedImageFiles(event.clipboardData);
          if (images.length > 0) {
            event.preventDefault();
            if (!imageAttachmentsAllowed) {
              onUnsupportedImageAttachment?.("This Agent does not accept images.");
              return;
            }
            if (!fileBrowser?.attachImage) {
              onUnsupportedImageAttachment?.("Images can be attached after the Task is open.");
              return;
            }
            const draft = { prompt: draftRef.current, context: attachments };
            void attachEveryImage(images, (image) => fileBrowser.attachImage(image, draft)).catch((error: unknown) => {
              onUnsupportedImageAttachment?.(composerErrorMessage(error, "Unable to attach image."));
            });
            return;
          }
          const text = event.clipboardData?.getData?.("text/plain");
          if (!text) return;
          event.preventDefault();
          insertEditorText(text);
        }}
        onKeyDown={(event) => {
          if (filePicker) {
            if (event.key === "ArrowDown" && filePicker.paths.length > 0) {
              event.preventDefault();
              setFilePicker((current) => current ? {
                ...current,
                activeIndex: (current.activeIndex + 1) % current.paths.length,
              } : current);
              return;
            }
            if (event.key === "ArrowUp" && filePicker.paths.length > 0) {
              event.preventDefault();
              setFilePicker((current) => current ? {
                ...current,
                activeIndex: (current.activeIndex - 1 + current.paths.length) % current.paths.length,
              } : current);
              return;
            }
            if ((event.key === "Tab" || event.key === "Enter") && filePicker.paths.length > 0) {
              event.preventDefault();
              selectFileMention(filePicker.paths[filePicker.activeIndex]);
              return;
            }
            if (event.key === "Escape") {
              setFileMentionToken(undefined);
              return;
            }
          }
          if (slashPicker) {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setSlashPicker((current) => current ? {
                ...current,
                activeIndex: (current.activeIndex + 1) % current.results.length,
              } : current);
              return;
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setSlashPicker((current) => current ? {
                ...current,
                activeIndex: (current.activeIndex - 1 + current.results.length) % current.results.length,
              } : current);
              return;
            }
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              selectSlashCommand(slashPicker.results[slashPicker.activeIndex]);
              return;
            }
            if (event.key === "Escape") {
              setSlashPicker(undefined);
              return;
            }
          }
          const mobileComposerBehavior = usesMobileComposerBehavior();
          if (!mobileComposerBehavior && shouldSubmitComposerKey(event, submitShortcut) && canSubmit) {
            event.preventDefault();
            submitDraft();
            return;
          }
          if (shouldInsertComposerNewline(event, submitShortcut)) {
            event.preventDefault();
            insertEditorText("\n");
            return;
          }
          if (event.key === "Enter") {
            event.preventDefault();
            insertEditorText("\n");
          }
        }}
        placeholder={availability.placeholder}
        ref={editorRef}
        renderRevision={editorRenderRevision}
        value={editorText}
      />
      {slashPicker ? (
        <SlashCommandPicker
          activeIndex={slashPicker.activeIndex}
          commands={slashPicker.results}
          onSelect={selectSlashCommand}
        />
      ) : null}
      {filePicker ? <FileMentionPicker onSelect={selectFileMention} state={filePicker} /> : null}
      {error ? <p className="inline-error">{error}</p> : null}
      {hasDraftContent && !canSubmit && availability.submissionBlockedMessage ? (
        <p aria-live="polite" className="inline-status composer-submission-blocker">
          {availability.submissionBlockedMessage}
        </p>
      ) : null}
      {showSlowConfigUpdate && configMutationId ? (
        <p aria-live="polite" className="inline-status composer-config-update-status">
          <LoaderCircle aria-hidden="true" className="composer-config-pending" size={13} />
          <span>Agent is still updating options…</span>
        </p>
      ) : null}
      <div className="composer-footer">
        <ComposerControls
          agentLocked={agentLocked}
          agents={agents}
          configLocked={configLocked || optimisticConfigChange !== undefined}
          configOptions={presentedConfigOptions}
          disabled={disabled}
          fileBrowser={fileBrowser}
          fileDropHandlerRef={fileDropHandlerRef}
          attachmentCount={attachments.filter((attachment) => attachment.kind !== "image").length}
          imageAttachmentsAllowed={imageAttachmentsAllowed}
          onFileUploadsChange={setFileUploads}
          onUnsupportedImageAttachment={onUnsupportedImageAttachment}
          onSelectAgent={onSelectAgent}
          onSelectConfigOption={selectConfigOption}
          onSelectIsolation={onSelectIsolation}
          openMenu={openMenu}
          setOpenMenu={setOpenMenu}
          selectAndClose={selectAndClose}
          selection={selection}
          toggleMenu={toggleMenu}
          showAgentSelector={showAgentSelector}
          showIsolationSelector={showIsolationSelector}
        />
        <div className="composer-actions">
          {availability.submitting ? (
            <span aria-label={availability.submitPendingLabel} className="composer-submit-pending">
              <LoaderCircle size={14} aria-hidden="true" />
            </span>
          ) : null}
          {showStopAction && onCancel ? (
            <IconButton ariaLabel="Stop task" className="composer-stop-button" icon={<CircleStop size={14} />} onClick={onCancel} />
          ) : null}
          {!availability.submitting && showSendAction ? (
            <IconButton
              ariaLabel={availability.submitActionLabel}
              className="composer-send-button"
              disabled={!canSubmit}
              icon={<ArrowUp size={15} />}
              onClick={submitDraft}
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}

function configValueEquals(left: ConfigOptionCurrentValue, right: ConfigOptionCurrentValue) {
  return left.type === right.type && left.value === right.value;
}
