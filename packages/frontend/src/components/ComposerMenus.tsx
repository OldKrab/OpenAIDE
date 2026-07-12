import { ChevronLeft, Code2, Cpu, Image, Paperclip, Plus, Shield, SlidersHorizontal } from "lucide-react";
import type { ConfigOption, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { useRef, type Dispatch, type SetStateAction } from "react";
import {
  agentOptions,
  isolationOptions,
  type AgentOption,
  type ComposerSelection,
} from "../state/composerOptions";
import { AgentIcon } from "./AgentIcon";
import { ComposerFileBrowser } from "./ComposerFileBrowser";
import { IconButton, MenuButton, Popover, PopoverBackButton, Selector } from "./ComposerPrimitives";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import { attachEveryImage } from "./imageAttachmentBatch";

export type ComposerMenu = "add" | "files" | "agent" | "options" | "isolation" | `config:${string}`;

type ComposerControlsProps = {
  agentLocked: boolean;
  agents?: AgentOption[];
  configLocked: boolean;
  configOptions?: ConfigOptionsCatalog;
  disabled: boolean;
  fileBrowser?: TaskFileBrowserCallbacks;
  onUnsupportedImageAttachment?: (message?: string) => void;
  onSelectAgent?: (agentId: string) => void;
  onSelectConfigOption?: (configId: string, value: string) => void;
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
  const optionControls = configOptions?.options ?? [];
  const mobileOptionsLabel = compactRunOptionsLabel(optionControls, selection.isolation, showIsolationSelector);
  const imageUploadRef = useRef<HTMLInputElement | null>(null);
  const uploadImages = (files: File[], input: HTMLInputElement) => {
    input.value = "";
    if (files.length === 0 || !fileBrowser?.attachPastedImage) return;
    void attachEveryImage(files, (file) => fileBrowser.attachPastedImage(file)).then(
      () => setOpenMenu(undefined),
      (error: unknown) => onUnsupportedImageAttachment?.(errorMessage(error, "Unable to upload image.")),
    );
  };

  return (
    <div className="composer-controls">
      <div className="composer-menu-anchor">
        <IconButton
          ariaLabel="Add context"
          disabled={disabled}
          icon={<Plus size={14} />}
          onClick={() => toggleMenu("add")}
          pressed={openMenu === "add"}
        />
        {openMenu === "add" ? (
          <Popover label="Add context">
            <MenuButton
              description="Browse files and images in this workspace."
              disabled={!fileBrowser}
              icon={<Paperclip size={13} />}
              label="Workspace files"
              onClick={() => {
                if (fileBrowser) setOpenMenu("files");
              }}
            />
            <MenuButton
              description="Choose images from this device."
              disabled={!fileBrowser}
              icon={<Image size={13} />}
              label="Upload or photo"
              onClick={() => imageUploadRef.current?.click()}
            />
            <input
              accept="image/*"
              disabled={!fileBrowser}
              multiple
              onChange={(event) => uploadImages(Array.from(event.target.files ?? []), event.currentTarget)}
              ref={imageUploadRef}
              style={{ display: "none" }}
              type="file"
            />
          </Popover>
        ) : null}
        {openMenu === "files" && fileBrowser ? (
          <Popover className="composer-file-browser-popover" label="Workspace files">
            <ComposerFileBrowser browser={fileBrowser} onAttached={() => setOpenMenu(undefined)} />
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
      {optionControls.map((option) => {
        const menuId = configMenuId(option.id);
        return (
          <div className="composer-option-anchor composer-config-control-anchor" key={option.id}>
            <Selector
              className="composer-config-control"
              disabled={configLocked}
              icon={configIcon(option)}
              label={configOptionLabel(option)}
              locked={configLocked}
              menuOpen={openMenu === menuId}
              onClick={() => toggleMenu(menuId)}
            />
            {openMenu === menuId ? (
              <Popover className="composer-model-menu" label={option.label}>
                <PopoverBackButton
                  ariaLabel="Back to options"
                  icon={<ChevronLeft size={13} />}
                  label={option.label}
                  onClick={() => setOpenMenu("options")}
                />
                {option.values.map((value) => (
                  <MenuButton
                    active={option.current_value === value.id}
                    description={value.description ?? value.group_label ?? option.description ?? ""}
                    icon={configIcon(option, 13)}
                    key={value.id}
                    label={value.label}
                    onClick={() => selectAndClose(() => onSelectConfigOption?.(option.id, value.id))}
                  />
                ))}
              </Popover>
            ) : null}
          </div>
        );
      })}
      {showIsolationSelector ? (
        <div className="composer-option-anchor composer-isolation-control-anchor">
          <Selector
            className="composer-isolation-control"
            disabled={controlsLocked}
            icon={<Shield size={12} />}
            label={isolationLabel(selection.isolation)}
            locked={agentLocked}
            menuOpen={openMenu === "isolation"}
            onClick={() => toggleMenu("isolation")}
          />
          {openMenu === "isolation" ? (
            <Popover label="Isolation">
              <PopoverBackButton
                ariaLabel="Back to options"
                icon={<ChevronLeft size={13} />}
                label="Isolation"
                onClick={() => setOpenMenu("options")}
              />
              {isolationOptions.map((isolation) => (
                <MenuButton
                  active={selection.isolation === isolation.id}
                  description={isolation.description}
                  icon={<Shield size={13} />}
                  key={isolation.id}
                  label={isolation.label}
                  onClick={() => selectAndClose(() => onSelectIsolation?.(isolation.id))}
                />
              ))}
            </Popover>
          ) : null}
        </div>
      ) : null}
      {optionControls.length > 0 || showIsolationSelector ? (
        <div className="composer-mobile-options-anchor">
          <Selector
            disabled={disabled}
            icon={<SlidersHorizontal size={12} />}
            label={mobileOptionsLabel}
            locked={false}
            menuOpen={openMenu === "options"}
            onClick={() => toggleMenu("options")}
          />
          {openMenu === "options" ? (
            <Popover label="Run options">
              {optionControls.map((option) => (
                <MenuButton
                  description={runOptionDescription(option)}
                  icon={configIcon(option, 13)}
                  key={option.id}
                  label={runOptionLabel(option)}
                  onClick={() => setOpenMenu(configMenuId(option.id))}
                />
              ))}
              {showIsolationSelector ? (
                <MenuButton
                  description={`Current: ${isolationLabel(selection.isolation)}`}
                  icon={<Shield size={13} />}
                  label="Isolation"
                  onClick={() => setOpenMenu("isolation")}
                />
              ) : null}
            </Popover>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function isolationLabel(isolation: IsolationKind) {
  return isolationOptions.find((option) => option.id === isolation)?.label ?? isolation;
}

function compactRunOptionsLabel(
  options: ConfigOption[],
  isolation: IsolationKind,
  showIsolation: boolean,
) {
  const primaryOption = options.find((option) => option.category === "model" || option.id === "model") ?? options[0];
  const selectedValue = primaryOption ? configOptionLabel(primaryOption) : undefined;
  const detail = selectedValue || (showIsolation ? isolationLabel(isolation) : undefined);
  return detail ? `Options · ${detail}` : "Options";
}

function configMenuId(optionId: string): ComposerMenu {
  return `config:${optionId}`;
}

function configOptionLabel(option: ConfigOption) {
  const selected = option.values.find((value) => value.id === option.current_value);
  const valueLabel = normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(option.current_value) ?? option.label;
  const prefix = configOptionPrefix(option);
  if (!prefix) return valueLabel;
  return `${prefix}: ${valueLabel}`;
}

function runOptionLabel(option: ConfigOption) {
  if (option.category === "model") return "Model";
  if (option.category === "thought_level") return "Reasoning";
  return option.label.trim() || humanizeConfigValue(option.id) || "Option";
}

function runOptionDescription(option: ConfigOption) {
  const selected = option.values.find((value) => value.id === option.current_value);
  return `Current: ${normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(option.current_value) ?? option.current_value}`;
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function normalizedConfigValueLabel(label: string | undefined) {
  if (!label?.trim()) return undefined;
  const trimmed = label.trim();
  return trimmed === trimmed.toLowerCase() ? humanizeConfigValue(trimmed) ?? trimmed : trimmed;
}

function configOptionPrefix(option: ConfigOption) {
  if (option.category === "model") return undefined;
  const label = option.label.trim();
  if (!label) return undefined;
  if (option.category === "mode" || option.category === "thought_level") return undefined;
  return label.replace(/\s+mode$/i, "");
}

function humanizeConfigValue(value: string | undefined) {
  if (!value?.trim()) return undefined;
  const trimmed = value.trim();
  const modelMatch = /^(gpt|o|claude|gemini|llama|mistral|qwen|deepseek)([-_].+)$/i.exec(trimmed);
  if (modelMatch) {
    return `${modelPrefixLabel(modelMatch[1])}${modelMatch[2].replaceAll("_", "-")}`;
  }
  return trimmed
    .split(/[_-]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function modelPrefixLabel(prefix: string) {
  return prefix.length <= 3 ? prefix.toUpperCase() : prefix.charAt(0).toUpperCase() + prefix.slice(1);
}

function configIcon(option: ConfigOption, size = 12) {
  if (option.category === "model" || option.category === "thought_level") return <Cpu size={size} />;
  if (option.category === "mode") return <Code2 size={size} />;
  return <Shield size={size} />;
}
