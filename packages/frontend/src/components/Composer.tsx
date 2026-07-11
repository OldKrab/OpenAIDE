import { ArrowUp, CircleStop, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AgentCommandsCatalog, AgentSlashCommand, ComposerSubmitShortcut, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { agentOptions, type AgentOption, type ComposerAttachment, type ComposerSelection } from "../state/composerOptions";
import { ComposerAttachments } from "./ComposerAttachments";
import { ComposerControls, type ComposerMenu } from "./ComposerMenus";
import { ComposerEditor, type ComposerEditorHandle } from "./ComposerEditor";
import {
  composerErrorMessage,
  hasComposerContent,
  hasComposerText,
  pastedImageFiles,
} from "./composerDraftPolicy";
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

export { shouldInsertComposerNewline, shouldSubmitComposerKey } from "./composerKeymap";

type ComposerProps = {
  agentLocked?: boolean;
  attachments: ComposerAttachment[];
  autoFocus?: boolean;
  commandCatalog?: AgentCommandsCatalog;
  configLocked?: boolean;
  configOptions?: ConfigOptionsCatalog;
  disabled: boolean;
  error?: string;
  fileBrowser?: TaskFileBrowserCallbacks;
  focusRequestKey?: number | string;
  agents?: AgentOption[];
  onCancel?: () => void;
  onChange: (prompt: string) => void;
  onUnsupportedImageAttachment?: (message?: string) => void;
  onRevealAttachment?: (attachmentId: string) => Promise<void> | void;
  onRemoveAttachment: (attachmentId: string) => void;
  onSelectAgent?: (agentId: string) => void;
  onSelectConfigOption?: (configId: string, value: string) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  onSubmit: (prompt: string) => void;
  placeholder: string;
  prompt: string;
  selection: ComposerSelection;
  submitShortcut: ComposerSubmitShortcut;
  submitDisabled: boolean;
  submitRequiresText?: boolean;
  showTextRequirementError?: boolean;
  submitPending?: boolean;
  submitPendingLabel?: string;
  showAgentSelector?: boolean;
  showIsolationSelector?: boolean;
};

export function Composer({
  agentLocked = false,
  attachments,
  autoFocus = false,
  commandCatalog,
  configLocked = false,
  configOptions,
  disabled,
  error,
  fileBrowser,
  focusRequestKey,
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
  placeholder,
  prompt,
  selection,
  submitShortcut,
  submitDisabled,
  submitRequiresText = true,
  showTextRequirementError = true,
  submitPending = false,
  submitPendingLabel = "Task starting",
  showAgentSelector,
  showIsolationSelector,
}: ComposerProps) {
  const [openMenu, setOpenMenu] = useState<ComposerMenu | undefined>();
  const [slashPicker, setSlashPicker] = useState<SlashPickerState | undefined>();
  const [editorText, setEditorText] = useState(prompt);
  const [editorRenderRevision, setEditorRenderRevision] = useState(0);
  const [hasDraftContent, setHasDraftContent] = useState(() => hasComposerContent(prompt, attachments.length));
  const { keyboardFocus, onKeyboardNavigation, onPointerInteraction } = useComposerKeyboardFocus();
  const composerRef = useRef<HTMLElement | null>(null);
  const editorRef = useRef<ComposerEditorHandle | null>(null);
  const submitButtonRef = useRef<HTMLButtonElement | null>(null);
  const localErrorRef = useRef<HTMLParagraphElement | null>(null);
  const draftRef = useRef(prompt);
  const lastPromptRef = useRef(prompt);
  const commandCatalogRevision = commandCatalogKey(commandCatalog);
  const lastCommandCatalogKey = useRef(commandCatalogRevision);

  useComposerAutoFocus({ autoFocus, disabled, editorRef, focusRequestKey });

  useEffect(() => {
    if (!openMenu || typeof document === "undefined") return undefined;
    const onPointerDown = (event: PointerEvent) => {
      if (composerRef.current?.contains(event.target as Node)) return;
      setOpenMenu(undefined);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [openMenu]);

  useEffect(() => {
    if (!openMenu || typeof window === "undefined") return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpenMenu(undefined);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openMenu]);

  useEffect(() => {
    const promptChanged = prompt !== lastPromptRef.current;
    lastPromptRef.current = prompt;
    if (promptChanged) {
      draftRef.current = prompt;
      renderEditorText(prompt);
    }
    const draft = draftRef.current;
    setHasDraftContent(hasComposerContent(draft, attachments.length));
    syncSubmitButton(draft);
  }, [attachments.length, prompt, submitDisabled, submitRequiresText]);

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
    if (disabled && menu !== "add" && !menu.startsWith("config:")) return;
    commitDraft();
    setOpenMenu((current) => (current === menu ? undefined : menu));
  };

  const selectAndClose = (select: () => void) => {
    commitDraft();
    select();
    setOpenMenu(undefined);
  };

  const commitDraft = () => {
    onChange(draftRef.current);
  };

  const renderEditorText = (value: string) => {
    setEditorText(value);
    setEditorRenderRevision((revision) => revision + 1);
  };

  const syncDraft = (value: string, options: { renderEditor?: boolean } = {}) => {
    draftRef.current = value;
    if (options.renderEditor) renderEditorText(value);
    setHasDraftContent(hasComposerContent(value, attachments.length));
    syncSubmitButton(value);
  };

  const updateSlashPicker = (value: string, cursor: number) => {
    const picker = slashPickerForCatalog(commandCatalog, value, cursor);
    setSlashPicker(picker);
    if (picker) setOpenMenu(undefined);
  };

  const syncSubmitButton = (value: string) => {
    const button = submitButtonRef.current;
    const blocked = submitBlocked(value);
    if (button) button.disabled = blocked;
    const localError = localErrorRef.current;
    if (localError) localError.hidden = !localMessageShapeBlocked(value);
  };

  const submitBlocked = (value: string) =>
    submitDisabled || (submitRequiresText && !hasComposerText(value));
  const localMessageShapeBlocked = (value: string) =>
    !submitDisabled && submitRequiresText && !hasComposerText(value);

  const submitDraft = () => {
    const draft = draftRef.current;
    if (submitBlocked(draft)) return;
    onSubmit(draft);
    syncDraft("", { renderEditor: true });
    onChange("");
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

  const insertEditorText = (text: string) => {
    const selection = editorRef.current?.selectionRange() ?? {
      start: draftRef.current.length,
      end: draftRef.current.length,
    };
    const draft = draftRef.current;
    const nextText = `${draft.slice(0, selection.start)}${text}${draft.slice(selection.end)}`;
    const cursor = selection.start + text.length;
    syncDraft(nextText, { renderEditor: true });
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
      ref={composerRef}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          setOpenMenu(undefined);
        }
      }}
    >
      <ComposerAttachments
        attachments={attachments}
        disabled={disabled}
        onRemoveAttachment={onRemoveAttachment}
        onRevealAttachment={onRevealAttachment}
      />
      <ComposerEditor
        ariaLabel="Message"
        commandCatalog={commandCatalog}
        disabled={disabled}
        onBlur={commitDraft}
        onInputText={(value, cursor) => {
          syncDraft(value);
          updateSlashPicker(value, cursor);
        }}
        onPointerDown={() => {
          setOpenMenu(undefined);
          setSlashPicker(undefined);
        }}
        onPaste={(event) => {
          const images = pastedImageFiles(event.clipboardData);
          if (images.length > 0) {
            event.preventDefault();
            if (!fileBrowser?.attachPastedImage) {
              onUnsupportedImageAttachment?.("Images can be attached after the Task is open.");
              return;
            }
            const draft = { prompt: draftRef.current, context: attachments };
            commitDraft();
            void attachEveryImage(images, (image) => fileBrowser.attachPastedImage(image, draft)).catch((error: unknown) => {
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
          if (shouldSubmitComposerKey(event, submitShortcut) && !submitDisabled) {
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
        placeholder={placeholder}
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
      {error ? <p className="inline-error">{error}</p> : null}
      {!error && showTextRequirementError && attachments.length > 0 ? (
        <p className="inline-error" hidden={!localMessageShapeBlocked(draftRef.current)} ref={localErrorRef}>Add a message for this Agent.</p>
      ) : null}
      <div className="composer-footer">
        <ComposerControls
          agentLocked={agentLocked}
          agents={agents}
          configLocked={configLocked}
          configOptions={configOptions}
          disabled={disabled}
          fileBrowser={fileBrowser}
          onUnsupportedImageAttachment={onUnsupportedImageAttachment}
          onSelectAgent={onSelectAgent}
          onSelectConfigOption={onSelectConfigOption}
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
          {submitPending ? (
            <span aria-label={submitPendingLabel} className="composer-submit-pending">
              <LoaderCircle size={14} aria-hidden="true" />
            </span>
          ) : null}
          {!submitPending && onCancel && !hasDraftContent ? (
            <IconButton ariaLabel="Stop task" className="composer-stop-button" icon={<CircleStop size={14} />} onClick={onCancel} />
          ) : null}
          {!submitPending && (!onCancel || hasDraftContent) ? (
            <IconButton
              ariaLabel="Send message"
              className="composer-send-button"
              disabled={submitBlocked(draftRef.current)}
              icon={<ArrowUp size={15} />}
              onClick={submitDraft}
              ref={submitButtonRef}
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}
