import { Code2, Image, Plus } from "lucide-react";
import type { ConfigOptionCurrentValue, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { useRef, type Dispatch, type SetStateAction } from "react";
import {
  agentOptions,
  type AgentOption,
  type ComposerSelection,
} from "../state/composerOptions";
import { AgentIcon } from "./AgentIcon";
import { IconButton, MenuButton, Popover, Selector } from "./ComposerPrimitives";
import { ComposerRunOptions, type ComposerRunMenu } from "./ComposerRunOptions";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import { attachEveryImage } from "./imageAttachmentBatch";

export type ComposerMenu = "add" | "agent" | ComposerRunMenu;

type ComposerControlsProps = {
  agentLocked: boolean;
  agents?: AgentOption[];
  configLocked: boolean;
  configOptions?: ConfigOptionsCatalog;
  disabled: boolean;
  fileBrowser?: TaskFileBrowserCallbacks;
  imageAttachmentsAllowed: boolean;
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
  const uploadImages = (files: File[], input: HTMLInputElement) => {
    input.value = "";
    if (disabled || !imageAttachmentsAllowed || files.length === 0 || !fileBrowser?.attachImage) return;
    void attachEveryImage(files, (file) => fileBrowser.attachImage(file)).then(
      () => setOpenMenu(undefined),
      (error: unknown) => onUnsupportedImageAttachment?.(errorMessage(error, "Unable to upload image.")),
    );
  };

  return (
    <div className="composer-controls">
      <div className="composer-menu-anchor">
        <IconButton
          ariaLabel="Add context"
          disabled={disabled || !imageAttachmentsAllowed}
          icon={<Plus size={14} />}
          onClick={() => toggleMenu("add")}
          pressed={openMenu === "add"}
        />
        {openMenu === "add" ? (
          <Popover label="Add context">
            <MenuButton
              description="Choose images from this device."
              disabled={disabled || !imageAttachmentsAllowed || !fileBrowser}
              icon={<Image size={13} />}
              label="Upload or photo"
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
          </Popover>
        ) : null}
      </div>
      {showAgentSelector ? (
        <div className="composer-option-anchor">
          <Selector
            disabled={controlsLocked}
            icon={<Code2 size={12} />}
            label={selection.agentLabel}
            locked={agentLocked}
            menuOpen={openMenu === "agent"}
            onClick={() => toggleMenu("agent")}
          />
          {openMenu === "agent" ? (
            <Popover label="Agent">
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
            </Popover>
          ) : null}
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

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}
