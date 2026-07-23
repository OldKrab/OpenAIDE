import { Code2, FileUp, Image, Plus } from "lucide-react";
import type { ConfigOptionCurrentValue, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import {
  agentOptions,
  type AgentOption,
  type ComposerSelection,
} from "../state/composerOptions";
import { AgentIcon } from "./AgentIcon";
import { IconButton, MenuButton, Selector } from "./ComposerPrimitives";
import { ComposerRunOptions, type ComposerRunMenu } from "./ComposerRunOptions";
import { PopupMenu } from "./Popup";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import { attachEveryImage } from "./imageAttachmentBatch";
import type { ComposerFileUpload } from "./ComposerAttachments";
import { composerErrorMessage } from "./composerDraftPolicy";

export type ComposerMenu = "add" | "agent" | ComposerRunMenu;

type WebFileUploadItem = {
  id: string;
  file: File;
  controller: AbortController;
  dismiss(): void;
};

type ComposerControlsProps = {
  agentLocked: boolean;
  agents?: AgentOption[];
  configLocked: boolean;
  configOptions?: ConfigOptionsCatalog;
  disabled: boolean;
  fileBrowser?: TaskFileBrowserCallbacks;
  imageAttachmentsAllowed: boolean;
  attachmentCount: number;
  onFileUploadsChange: (uploads: ComposerFileUpload[]) => void;
  fileDropHandlerRef: MutableRefObject<((files: File[]) => void) | undefined>;
  onUnsupportedImageAttachment?: (message?: string) => void;
  onSelectAgent?: (agentId: string) => void;
  onSelectConfigOption?: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  openMenu?: ComposerMenu;
  selectAndClose: (select: () => void) => void;
  selection: ComposerSelection;
  setOpenMenu: Dispatch<SetStateAction<ComposerMenu | undefined>>;
  showAgentSelector?: boolean;
  showIsolationSelector?: boolean;
  toggleMenu: (menu: ComposerMenu) => void;
};

export function ComposerControls({
  agentLocked,
  agents = agentOptions,
  configLocked,
  configOptions,
  disabled,
  fileBrowser,
  imageAttachmentsAllowed,
  attachmentCount,
  onFileUploadsChange,
  fileDropHandlerRef,
  onUnsupportedImageAttachment,
  onSelectAgent,
  onSelectConfigOption,
  onSelectIsolation,
  openMenu,
  selectAndClose,
  selection,
  setOpenMenu,
  showAgentSelector = true,
  showIsolationSelector = true,
  toggleMenu,
}: ComposerControlsProps) {
  const controlsLocked = disabled || agentLocked;
  const configControlsLocked = disabled || configLocked;
  const imageUploadRef = useRef<HTMLInputElement | null>(null);
  const fileUploadRef = useRef<HTMLInputElement | null>(null);
  const fileAcquisitionActive = useRef(false);
  const uploadControllers = useRef(new Set<AbortController>());
  const [fileUploads, setFileUploads] = useState<ComposerFileUpload[]>([]);
  const uploadsActive = fileUploads.some((upload) => upload.state !== "error");
  useEffect(() => onFileUploadsChange(fileUploads), [fileUploads, onFileUploadsChange]);
  useEffect(() => () => {
    fileAcquisitionActive.current = false;
    for (const controller of uploadControllers.current) controller.abort();
    uploadControllers.current.clear();
  }, []);
  const uploadImages = (files: File[], input: HTMLInputElement) => {
    input.value = "";
    if (disabled || !imageAttachmentsAllowed || files.length === 0 || !fileBrowser?.attachImage) return;
    void attachEveryImage(files, (file) => fileBrowser.attachImage(file)).then(
      () => setOpenMenu(undefined),
      (error: unknown) => onUnsupportedImageAttachment?.(composerErrorMessage(error, "Unable to upload image.")),
    );
  };
  const attachFiles = (selectedFiles: File[]) => {
    if (disabled || !fileBrowser?.attachFiles) return;
    if (fileAcquisitionActive.current) {
      onUnsupportedImageAttachment?.("Wait for the current file selection to finish.");
      return;
    }
    const remaining = Math.max(0, 20 - attachmentCount);
    if (remaining === 0) {
      onUnsupportedImageAttachment?.("A draft can attach at most 20 files.");
      return;
    }
    if (fileBrowser.attachmentMode === "nativePicker") {
      fileAcquisitionActive.current = true;
      const id = `native-files-${Date.now()}`;
      const dismiss = () => setFileUploads((current) => current.filter((item) => item.id !== id));
      setFileUploads([{ id, label: "Choosing files…", loaded: 0, total: 0, state: "uploading", cancellable: false, cancel: () => undefined, dismiss }]);
      void fileBrowser.attachFiles([], {
        maxFiles: remaining,
        onProgress: () => undefined,
        signal: new AbortController().signal,
      }).then(dismiss, (error: unknown) => {
        setFileUploads((current) => current.map((item) => item.id === id
          ? { ...item, state: "error", error: composerErrorMessage(error, "Unable to attach files.") }
          : item));
      }).finally(() => {
        fileAcquisitionActive.current = false;
      });
      return;
    }
    if (selectedFiles.length === 0) return;
    const accepted = selectedFiles.slice(0, remaining);
    if (selectedFiles.length > accepted.length) {
      onUnsupportedImageAttachment?.("A draft can attach at most 20 files.");
    }
    const uploadFile = async (item: WebFileUploadItem) => {
      const controller = item.controller;
      uploadControllers.current.add(controller);
      setFileUploads((current) => current.map((upload) => upload.id === item.id
        ? { ...upload, loaded: 0, total: item.file.size, state: "uploading" }
        : upload));
      try {
        await fileBrowser.attachFiles?.([item.file], {
          maxFiles: 1,
          signal: controller.signal,
          onProgress: ({ loaded, total }) => setFileUploads((current) => current.map((upload) =>
            upload.id === item.id ? { ...upload, loaded, total } : upload)),
        });
        item.dismiss();
      } catch (error) {
        if (controller.signal.aborted) item.dismiss();
        else setFileUploads((current) => current.map((upload) => upload.id === item.id
          ? { ...upload, state: "error", error: composerErrorMessage(error, "Unable to upload file.") }
          : upload));
      } finally {
        uploadControllers.current.delete(controller);
      }
    };
    const retryUpload = (item: WebFileUploadItem) => {
      item.controller = new AbortController();
      fileAcquisitionActive.current = true;
      void uploadFile(item).finally(() => {
        if (uploadControllers.current.size === 0) fileAcquisitionActive.current = false;
      });
    };
    const queued = accepted.map((file, index): WebFileUploadItem => {
      const id = `file-upload-${Date.now()}-${index}`;
      const dismiss = () => setFileUploads((current) => current.filter((item) => item.id !== id));
      return {
        id,
        file,
        controller: new AbortController(),
        dismiss,
      };
    });
    fileAcquisitionActive.current = true;
    setFileUploads(queued.map((item) => ({
      id: item.id,
      label: item.file.name || "Attached file",
      loaded: 0,
      total: item.file.size,
      state: "queued",
      cancel: () => item.controller.abort(),
      dismiss: item.dismiss,
      retry: () => retryUpload(item),
    })));
    let next = 0;
    const worker = async () => {
      while (next < queued.length) {
        const item = queued[next++];
        if (item.controller.signal.aborted) {
          item.dismiss();
          continue;
        }
        await uploadFile(item);
      }
    };
    void Promise.all([worker(), worker()]).finally(() => {
      fileAcquisitionActive.current = false;
    });
  };
  fileDropHandlerRef.current = attachFiles;
  useEffect(() => () => {
    fileDropHandlerRef.current = undefined;
  }, [fileDropHandlerRef]);

  return (
    <div className="composer-controls">
      <div className="composer-menu-anchor">
        <PopupMenu
          className="composer-popover"
          label="Add context"
          onOpenChange={(nextOpen) => setOpenMenu(nextOpen ? "add" : undefined)}
          open={openMenu === "add"}
          placement="top-start"
          trigger={(popupTrigger) => (
            <IconButton
              ariaLabel="Add context"
              disabled={disabled || (!fileBrowser?.attachFiles && !imageAttachmentsAllowed)}
              icon={<Plus size={14} />}
              popupTrigger={popupTrigger}
              pressed={openMenu === "add"}
            />
          )}
        >
            <MenuButton
              description={fileBrowser?.attachmentMode === "nativePicker"
                ? "Choose files from this computer."
                : "Upload files to this task."}
              disabled={disabled || uploadsActive || !fileBrowser?.attachFiles || attachmentCount >= 20}
              icon={<FileUp size={13} />}
              label="Attach files"
              onClick={() => {
                if (fileBrowser?.attachmentMode === "nativePicker") attachFiles([]);
                else fileUploadRef.current?.click();
              }}
            />
            <input
              disabled={disabled || uploadsActive || !fileBrowser?.attachFiles}
              multiple
              onChange={(event) => {
                const files = Array.from(event.target.files ?? []);
                event.currentTarget.value = "";
                attachFiles(files);
              }}
              ref={fileUploadRef}
              style={{ display: "none" }}
              type="file"
            />
            <MenuButton
              description="Choose images from this device."
              disabled={disabled || !imageAttachmentsAllowed || !fileBrowser}
              icon={<Image size={13} />}
              label="Attach images"
              onClick={() => imageUploadRef.current?.click()}
            />
            <input
              accept="image/*"
              disabled={disabled || !imageAttachmentsAllowed || !fileBrowser}
              multiple
              onChange={(event) => uploadImages(Array.from(event.target.files ?? []), event.currentTarget)}
              ref={imageUploadRef}
              style={{ display: "none" }}
              type="file"
            />
        </PopupMenu>
      </div>
      {showAgentSelector ? (
        <div className="composer-option-anchor">
          <PopupMenu
            className="composer-popover"
            label="Agent"
            onOpenChange={(nextOpen) => setOpenMenu(nextOpen ? "agent" : undefined)}
            open={openMenu === "agent"}
            placement="top-start"
            trigger={(popupTrigger) => (
              <Selector
                disabled={controlsLocked}
                icon={<Code2 size={12} />}
                label={selection.agentLabel}
                locked={agentLocked}
                menuOpen={openMenu === "agent"}
                popupTrigger={popupTrigger}
              />
            )}
          >
              {agents.filter((agent) => agent.enabled !== false).map((agent) => (
                <MenuButton
                  active={selection.agentId === agent.id}
                  description={agent.description}
                  icon={<AgentIcon icon={agent.icon} size={13} />}
                  key={agent.id}
                  label={agent.label}
                  onClick={() => selectAndClose(() => onSelectAgent?.(agent.id))}
                />
              ))}
          </PopupMenu>
        </div>
      ) : null}
      <ComposerRunOptions
        configLocked={configControlsLocked}
        configOptions={configOptions}
        controlsLocked={controlsLocked}
        disabled={disabled}
        onSelectConfigOption={onSelectConfigOption}
        onSelectIsolation={onSelectIsolation}
        openMenu={openMenu}
        selectAndClose={selectAndClose}
        selection={selection}
        setOpenMenu={setOpenMenu}
        showIsolationSelector={showIsolationSelector}
        toggleMenu={toggleMenu}
      />
    </div>
  );
}
