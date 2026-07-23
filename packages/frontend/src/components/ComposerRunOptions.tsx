import { Brain, ChevronLeft, ChevronRight, Code2, Cpu, Shield, SlidersHorizontal } from "lucide-react";
import type { ConfigOption, ConfigOptionCurrentValue, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { useEffect, useId, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { isolationOptions, type ComposerSelection } from "../state/composerOptions";
import { MenuButton, PopoverBackButton, PopoverHeader, Selector } from "./ComposerPrimitives";
import { PopupMenu } from "./Popup";
import { useComposerOptionPacking } from "./useComposerOptionPacking";

export type ComposerRunMenu = "options" | "isolation" | `config:${string}`;
type ComposerMenu = "add" | "agent" | ComposerRunMenu;

type RunControl =
  | { kind: "config"; option: ConfigOption }
  | { kind: "isolation" };

type ComposerRunOptionsProps = {
  configLocked: boolean;
  configOptions?: ConfigOptionsCatalog;
  controlsLocked: boolean;
  disabled: boolean;
  onSelectConfigOption?: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  openMenu?: ComposerMenu;
  selectAndClose: (select: () => void) => void;
  selection: ComposerSelection;
  setOpenMenu: Dispatch<SetStateAction<ComposerMenu | undefined>>;
  showIsolationSelector: boolean;
  toggleMenu: (menu: ComposerMenu) => void;
};

/** Renders the Agent-owned controls directly until their trailing suffix no longer fits. */
export function ComposerRunOptions({
  configLocked,
  configOptions,
  controlsLocked,
  disabled,
  onSelectConfigOption,
  onSelectIsolation,
  openMenu,
  selectAndClose,
  selection,
  setOpenMenu,
  showIsolationSelector,
  toggleMenu,
}: ComposerRunOptionsProps) {
  const [optionHoverActive, setOptionHoverActive] = useState(false);
  const options = configOptions?.options ?? [];
  const pendingChange = configOptions?.pending_change;
  const controls: RunControl[] = [
    ...options.map((option): RunControl => ({ kind: "config", option })),
    ...(showIsolationSelector ? [{ kind: "isolation" } satisfies RunControl] : []),
  ];
  const measurementKey = controls.map((control) => controlMeasurementKey(control, pendingChange, selection)).join("|");
  const packing = useComposerOptionPacking(controls.length, measurementKey);
  const visibleControls = controls.slice(0, packing.visibleCount);
  const hiddenControls = controls.slice(packing.visibleCount);
  const hiddenMenuControl = openMenu
    ? hiddenControls.find((control) => menuForControl(control) === openMenu)
    : undefined;
  const overflowLocked = hiddenControls.length > 0 && hiddenControls.every((control) =>
    control.kind === "config" ? configLocked : controlsLocked);

  if (controls.length === 0) return null;

  return (
    <div
      className={`composer-adaptive-options${pendingChange ? " mutation-pending" : ""}`}
      onPointerDownCapture={() => setOptionHoverActive(false)}
      onPointerLeave={() => setOptionHoverActive(false)}
      ref={packing.containerRef}
    >
      {visibleControls.map((control) => (
        <DirectRunControl
          configLocked={configLocked}
          control={control}
          controlsLocked={controlsLocked}
          key={controlKey(control)}
          hoverSequenceActive={optionHoverActive}
          onHoverSequenceActivate={() => setOptionHoverActive(true)}
          onHoverSequenceReset={() => setOptionHoverActive(false)}
          onSelectConfigOption={onSelectConfigOption}
          onSelectIsolation={onSelectIsolation}
          openMenu={openMenu}
          pendingChange={pendingChange}
          selectAndClose={selectAndClose}
          selection={selection}
          toggleMenu={toggleMenu}
        />
      ))}
      {hiddenControls.length > 0 ? (
        <div className="composer-option-anchor composer-overflow-options-anchor">
          <PopupMenu
            className="composer-popover composer-overflow-menu"
            label={hiddenMenuControl ? controlLabel(hiddenMenuControl) : "More options"}
            onOpenChange={(nextOpen) => setOpenMenu(nextOpen ? "options" : undefined)}
            open={openMenu === "options" || hiddenMenuControl !== undefined}
            placement="top-start"
            trigger={(popupTrigger) => (
              <Selector
                className="composer-overflow-options-control"
                disabled={disabled || overflowLocked}
                icon={<SlidersHorizontal size={12} />}
                label={`More · ${hiddenControls.length}`}
                locked={overflowLocked}
                menuOpen={openMenu === "options" || hiddenMenuControl !== undefined}
                pending={hiddenControls.some((control) =>
                  control.kind === "config" && pendingChange?.option_id === control.option.id)}
                popupTrigger={popupTrigger}
              />
            )}
          >
            {openMenu === "options" ? (
              <>
              {hiddenControls.map((control) => (
                control.kind === "config" && control.option.kind === "boolean" ? (
                  <BooleanConfigControl
                    compact={false}
                    disabled={configLocked}
                    key={controlKey(control)}
                    onToggle={() => onSelectConfigOption?.(
                      control.option.id,
                      { type: "boolean", value: !displayedBooleanValue(control.option, pendingChange) },
                    )}
                    option={control.option}
                    pendingValue={pendingBooleanValue(control.option, pendingChange)}
                  />
                ) : (
                  <MenuButton
                    className="composer-overflow-menu-row"
                    description={controlDescription(control, pendingChange, selection)}
                    disabled={control.kind === "config" ? configLocked : controlsLocked}
                    endIcon={<ChevronRight size={12} />}
                    icon={control.kind === "config" ? undefined : controlIcon(control, 13)}
                    key={controlKey(control)}
                    label={controlLabel(control)}
                    onClick={() => {
                      const menu = menuForControl(control);
                      if (menu) setOpenMenu(menu);
                    }}
                  />
                )
              ))}
              </>
            ) : hiddenMenuControl ? (
              <GroupedControlMenu
                configLocked={configLocked}
                control={hiddenMenuControl}
                controlsLocked={controlsLocked}
                onSelectConfigOption={onSelectConfigOption}
                onSelectIsolation={onSelectIsolation}
                selection={selection}
                setOpenMenu={setOpenMenu}
              />
            ) : null}
          </PopupMenu>
        </div>
      ) : null}
      {packing.measurementAvailable ? (
        <MeasurementSurface
          configLocked={configLocked}
          controls={controls}
          controlsLocked={controlsLocked}
          pendingChange={pendingChange}
          selection={selection}
          {...packing}
        />
      ) : null}
    </div>
  );
}

function DirectRunControl({
  configLocked,
  control,
  controlsLocked,
  hoverSequenceActive,
  onHoverSequenceActivate,
  onHoverSequenceReset,
  onSelectConfigOption,
  onSelectIsolation,
  openMenu,
  pendingChange,
  selectAndClose,
  selection,
  toggleMenu,
}: {
  configLocked: boolean;
  control: RunControl;
  controlsLocked: boolean;
  hoverSequenceActive: boolean;
  onHoverSequenceActivate: () => void;
  onHoverSequenceReset: () => void;
  onSelectConfigOption?: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  openMenu?: ComposerMenu;
  pendingChange?: NonNullable<ConfigOptionsCatalog["pending_change"]>;
  selectAndClose: (select: () => void) => void;
  selection: ComposerSelection;
  toggleMenu: (menu: ComposerMenu) => void;
}) {
  const infoId = useId();
  const [pointerHoverArmed, setPointerHoverArmed] = useState(false);
  const hoverActivationTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(hoverActivationTimer.current), []);
  const clearHoverActivation = () => {
    clearTimeout(hoverActivationTimer.current);
    hoverActivationTimer.current = undefined;
  };
  const hoverOwnerProps = {
    "data-hover-armed": pointerHoverArmed || undefined,
    "data-hover-quick": hoverSequenceActive || undefined,
    onPointerDown: () => {
      clearHoverActivation();
      setPointerHoverArmed(false);
      onHoverSequenceReset();
    },
    onPointerLeave: clearHoverActivation,
    onPointerMove: () => {
      setPointerHoverArmed(true);
      if (hoverSequenceActive || hoverActivationTimer.current) return;
      hoverActivationTimer.current = setTimeout(() => {
        hoverActivationTimer.current = undefined;
        onHoverSequenceActivate();
      }, 600);
    },
  };
  const optionInfo = control.kind === "config" ? {
    description: control.option.description,
    label: control.option.label.trim() || controlLabel(control),
  } : undefined;
  if (control.kind === "config" && control.option.kind === "boolean") {
    return (
      <div className="composer-option-anchor composer-config-control-anchor" {...hoverOwnerProps}>
        <BooleanConfigControl
          compact
          describedBy={infoId}
          disabled={configLocked}
          onToggle={() => onSelectConfigOption?.(
            control.option.id,
            { type: "boolean", value: !displayedBooleanValue(control.option, pendingChange) },
          )}
          option={control.option}
          pendingValue={pendingBooleanValue(control.option, pendingChange)}
        />
        <OptionInfoTooltip
          description={control.option.description}
          hidden={openMenu !== undefined}
          id={infoId}
          label={control.option.label.trim() || controlLabel(control)}
        />
      </div>
    );
  }
  const menu = menuForControl(control);
  const locked = control.kind === "config" ? configLocked : controlsLocked;
  const pending = control.kind === "config" && pendingChange?.option_id === control.option.id;
  return (
    <div
      className={`composer-option-anchor ${control.kind === "config" ? "composer-config-control-anchor" : "composer-isolation-control-anchor"}`}
      {...hoverOwnerProps}
    >
      {menu ? (
        <PopupMenu
          className="composer-popover composer-model-menu"
          label={controlLabel(control)}
          onOpenChange={(nextOpen) => {
            if ((openMenu === menu) !== nextOpen) toggleMenu(menu);
          }}
          open={openMenu === menu}
          placement="top-start"
          trigger={(popupTrigger) => (
            <Selector
              className={control.kind === "config" ? "composer-config-control" : "composer-isolation-control"}
              describedBy={optionInfo ? infoId : undefined}
              disabled={locked}
              icon={control.kind === "config" ? undefined : controlIcon(control)}
              label={controlDirectLabel(control, pending ? pendingChange?.requested_value : undefined, selection)}
              locked={locked}
              menuOpen={openMenu === menu}
              pending={pending}
              popupTrigger={popupTrigger}
            />
          )}
        >
          <ControlValueMenuContent
            configLocked={configLocked}
            control={control}
            controlsLocked={controlsLocked}
            onSelectConfigOption={(optionId, value) =>
              selectAndClose(() => onSelectConfigOption?.(optionId, value))}
            onSelectIsolation={(isolation) =>
              selectAndClose(() => onSelectIsolation?.(isolation))}
            selection={selection}
          />
        </PopupMenu>
      ) : null}
      {optionInfo ? (
        <OptionInfoTooltip
          description={optionInfo.description}
          hidden={openMenu !== undefined}
          id={infoId}
          label={optionInfo.label}
        />
      ) : null}
    </div>
  );
}

function GroupedControlMenu({
  configLocked,
  control,
  controlsLocked,
  onSelectConfigOption,
  onSelectIsolation,
  selection,
  setOpenMenu,
}: {
  configLocked: boolean;
  control: RunControl;
  controlsLocked: boolean;
  onSelectConfigOption?: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  selection: ComposerSelection;
  setOpenMenu: Dispatch<SetStateAction<ComposerMenu | undefined>>;
}) {
  return (
    <ControlValueMenuContent
      configLocked={configLocked}
      control={control}
      controlsLocked={controlsLocked}
      onBack={() => setOpenMenu("options")}
      onSelectConfigOption={(optionId, value) => {
        onSelectConfigOption?.(optionId, value);
        setOpenMenu("options");
      }}
      onSelectIsolation={(isolation) => {
        onSelectIsolation?.(isolation);
        setOpenMenu("options");
      }}
      selection={selection}
    />
  );
}

function ControlValueMenuContent({
  configLocked,
  control,
  controlsLocked,
  onBack,
  onSelectConfigOption,
  onSelectIsolation,
  selection,
}: {
  configLocked: boolean;
  control: RunControl;
  controlsLocked: boolean;
  onBack?: () => void;
  onSelectConfigOption: (configId: string, value: ConfigOptionCurrentValue) => void;
  onSelectIsolation: (isolation: IsolationKind) => void;
  selection: ComposerSelection;
}) {
  const label = controlLabel(control);
  const optionLabel = control.kind === "config" ? control.option.label.trim() || label : label;
  const optionDescription = control.kind === "config" ? control.option.description : undefined;
  return (
    <>
      {onBack ? (
        <PopoverBackButton ariaLabel="Back to options" description={optionDescription} icon={<ChevronLeft size={13} />} label={optionLabel} onClick={onBack} />
      ) : null}
      {control.kind === "config" && !onBack ? <PopoverHeader description={optionDescription} label={optionLabel} /> : null}
      {control.kind === "config" ? control.option.values.map((value) => (
        <MenuButton
          active={currentId(control.option) === value.id}
          description={value.description ?? value.group_label ?? control.option.description ?? ""}
          disabled={configLocked}
          icon={undefined}
          key={value.id}
          label={value.label}
          onClick={() => onSelectConfigOption(control.option.id, { type: "id", value: value.id })}
        />
      )) : isolationOptions.map((isolation) => (
        <MenuButton
          active={selection.isolation === isolation.id}
          description={isolation.description}
          disabled={controlsLocked}
          icon={<Shield size={13} />}
          key={isolation.id}
          label={isolation.label}
          onClick={() => onSelectIsolation(isolation.id)}
        />
      ))}
    </>
  );
}

function MeasurementSurface({
  configLocked,
  controls,
  controlsLocked,
  measurementSurfaceRef,
  pendingChange,
  selection,
  setOptionMeasureRef,
  setOverflowMeasureRef,
}: {
  configLocked: boolean;
  controls: RunControl[];
  controlsLocked: boolean;
  measurementSurfaceRef: React.RefObject<HTMLDivElement | null>;
  pendingChange?: NonNullable<ConfigOptionsCatalog["pending_change"]>;
  selection: ComposerSelection;
  setOptionMeasureRef: (index: number, node: HTMLDivElement | null) => void;
  setOverflowMeasureRef: (hiddenCount: number, node: HTMLDivElement | null) => void;
}) {
  return (
    <div aria-hidden="true" className="composer-options-measurement" inert ref={measurementSurfaceRef}>
      {controls.map((control, index) => {
        const pending = control.kind === "config" && pendingChange?.option_id === control.option.id;
        const locked = control.kind === "config" ? configLocked : controlsLocked;
        return (
          <div className={`composer-option-anchor ${control.kind === "config" ? "composer-config-control-anchor" : ""}`} key={controlKey(control)} ref={(node) => setOptionMeasureRef(index, node)}>
            {control.kind === "config" && control.option.kind === "boolean" ? (
              <BooleanConfigControl
                compact
                disabled={locked}
                onToggle={() => {}}
                option={control.option}
                pendingValue={pendingBooleanValue(control.option, pendingChange)}
              />
            ) : (
              <Selector
                disabled={locked}
                icon={control.kind === "config" ? undefined : controlIcon(control)}
                label={controlDirectLabel(control, pending ? pendingChange?.requested_value : undefined, selection)}
                locked={locked}
                menuOpen={false}
                onClick={() => {}}
                pending={pending}
              />
            )}
          </div>
        );
      })}
      {controls.map((_, index) => {
        const hiddenCount = index + 1;
        return (
          <div className="composer-option-anchor" key={hiddenCount} ref={(node) => setOverflowMeasureRef(hiddenCount, node)}>
            <Selector
              disabled={false}
              icon={<SlidersHorizontal size={12} />}
              label={`More · ${hiddenCount}`}
              locked={false}
              menuOpen={false}
              onClick={() => {}}
            />
          </div>
        );
      })}
    </div>
  );
}

function menuForControl(control: RunControl): ComposerRunMenu | undefined {
  if (control.kind === "config" && control.option.kind === "boolean") return undefined;
  return control.kind === "config" ? `config:${control.option.id}` : "isolation";
}

function controlKey(control: RunControl) {
  return control.kind === "config" ? `config:${control.option.id}` : "isolation";
}

function controlLabel(control: RunControl) {
  return control.kind === "config" ? runOptionLabel(control.option) : "Isolation";
}

function controlDescription(
  control: RunControl,
  pendingChange: ConfigOptionsCatalog["pending_change"],
  selection: ComposerSelection,
) {
  if (control.kind === "isolation") return `Current: ${isolationLabel(selection.isolation)}`;
  const displayedValue = pendingChange?.option_id === control.option.id
    ? pendingChange.requested_value
    : control.option.current_value;
  if (displayedValue.type === "boolean") return `Current: ${displayedValue.value ? "On" : "Off"}`;
  const selected = control.option.values.find((value) => value.id === displayedValue.value);
  return `Current: ${normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(displayedValue.value) ?? displayedValue.value}`;
}

function controlDirectLabel(control: RunControl, displayedValue: ConfigOptionCurrentValue | undefined, selection: ComposerSelection) {
  return control.kind === "config"
    ? configOptionLabel(control.option, displayedValue)
    : isolationLabel(selection.isolation);
}

function controlIcon(control: RunControl, size = 12) {
  return control.kind === "config" ? configIcon(control.option, size) : <Shield size={size} />;
}

function controlMeasurementKey(
  control: RunControl,
  pendingChange: ConfigOptionsCatalog["pending_change"],
  selection: ComposerSelection,
) {
  return `${controlKey(control)}:${controlDirectLabel(
    control,
    control.kind === "config" && pendingChange?.option_id === control.option.id
      ? pendingChange.requested_value
      : undefined,
    selection,
  )}`;
}

function isolationLabel(isolation: IsolationKind) {
  return isolationOptions.find((option) => option.id === isolation)?.label ?? isolation;
}

function configOptionLabel(option: ConfigOption, displayedValue = option.current_value) {
  if (displayedValue.type === "boolean") return option.label;
  const selected = option.values.find((value) => value.id === displayedValue.value);
  return normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(displayedValue.value) ?? option.label;
}

function currentId(option: ConfigOption) {
  return option.current_value.type === "id" ? option.current_value.value : undefined;
}

function pendingBooleanValue(
  option: ConfigOption,
  pendingChange: ConfigOptionsCatalog["pending_change"],
) {
  if (pendingChange?.option_id !== option.id || pendingChange.requested_value.type !== "boolean") {
    return undefined;
  }
  return pendingChange.requested_value.value;
}

function displayedBooleanValue(
  option: ConfigOption,
  pendingChange: ConfigOptionsCatalog["pending_change"],
) {
  return pendingBooleanValue(option, pendingChange)
    ?? (option.current_value.type === "boolean" ? option.current_value.value : false);
}

function BooleanConfigControl({
  compact,
  describedBy,
  disabled,
  onToggle,
  option,
  pendingValue,
}: {
  compact: boolean;
  describedBy?: string;
  disabled: boolean;
  onToggle: () => void;
  option: ConfigOption;
  pendingValue?: boolean;
}) {
  const displayedValue = pendingValue
    ?? (option.current_value.type === "boolean" ? option.current_value.value : false);
  const pending = pendingValue !== undefined;
  return (
    <button
      aria-busy={pending || undefined}
      aria-checked={displayedValue}
      aria-describedby={describedBy}
      aria-label={`${option.label}: ${displayedValue ? "On" : "Off"}${pending ? ", updating Agent option" : ""}`}
      className={`${compact ? "composer-boolean-control" : "composer-overflow-boolean-control"}${pending ? " pending" : ""}`}
      disabled={disabled}
      onClick={onToggle}
      role="switch"
      type="button"
    >
      {compact ? (
        <span className="composer-boolean-control-content">
          <span className="composer-boolean-label">{option.label}</span>
          <span aria-hidden="true" className={`composer-boolean-indicator${displayedValue ? " checked" : ""}`} />
        </span>
      ) : (
        <>
          <span className="composer-boolean-copy">
            <span className="composer-boolean-heading">
              <strong>{option.label}</strong>
              <span aria-hidden="true" className={`composer-boolean-indicator${displayedValue ? " checked" : ""}`} />
            </span>
            {option.description ? <small>{option.description}</small> : null}
          </span>
        </>
      )}
    </button>
  );
}

function OptionInfoTooltip({
  description,
  hidden = false,
  id,
  label,
}: {
  description?: string;
  hidden?: boolean;
  id: string;
  label: string;
}) {
  return (
    <div aria-hidden={hidden || undefined} className={`composer-option-info${hidden ? " hidden" : ""}`} id={id} role="tooltip">
      <strong>{label}</strong>
      {description ? <small>{description}</small> : null}
    </div>
  );
}

function runOptionLabel(option: ConfigOption) {
  if (option.category === "model") return "Model";
  if (option.category === "thought_level") return "Reasoning";
  return option.label.trim() || humanizeConfigValue(option.id) || "Option";
}

function normalizedConfigValueLabel(label: string | undefined) {
  if (!label?.trim()) return undefined;
  const trimmed = label.trim();
  return trimmed === trimmed.toLowerCase() ? humanizeConfigValue(trimmed) ?? trimmed : trimmed;
}

function humanizeConfigValue(value: string | undefined) {
  if (!value?.trim()) return undefined;
  const trimmed = value.trim();
  const modelMatch = /^(gpt|o|claude|gemini|llama|mistral|qwen|deepseek)([-_].+)$/i.exec(trimmed);
  if (modelMatch) return `${modelPrefixLabel(modelMatch[1])}${modelMatch[2].replaceAll("_", "-")}`;
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
  if (option.category === "model") return <Cpu size={size} />;
  if (option.category === "thought_level") return <Brain size={size} />;
  if (option.category === "mode") return <Code2 size={size} />;
  return <SlidersHorizontal size={size} />;
}
