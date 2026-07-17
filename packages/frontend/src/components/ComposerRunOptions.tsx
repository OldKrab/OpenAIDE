import { Brain, ChevronLeft, ChevronRight, Code2, Cpu, Shield, SlidersHorizontal } from "lucide-react";
import type { ConfigOption, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import type { Dispatch, SetStateAction } from "react";
import { isolationOptions, type ComposerSelection } from "../state/composerOptions";
import { MenuButton, Popover, PopoverBackButton, Selector } from "./ComposerPrimitives";
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
  onSelectConfigOption?: (configId: string, value: string) => void;
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
  const hiddenMenuControl = hiddenControls.find((control) => menuForControl(control) === openMenu);
  const overflowLocked = hiddenControls.length > 0 && hiddenControls.every((control) =>
    control.kind === "config" ? configLocked : controlsLocked);

  if (controls.length === 0) return null;

  return (
    <div className="composer-adaptive-options" ref={packing.containerRef}>
      {visibleControls.map((control) => (
        <DirectRunControl
          configLocked={configLocked}
          control={control}
          controlsLocked={controlsLocked}
          key={controlKey(control)}
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
          <Selector
            className="composer-overflow-options-control"
            disabled={disabled || overflowLocked}
            icon={<SlidersHorizontal size={12} />}
            label={`More · ${hiddenControls.length}`}
            locked={overflowLocked}
            menuOpen={openMenu === "options" || hiddenMenuControl !== undefined}
            onClick={() => toggleMenu("options")}
            pending={hiddenControls.some((control) =>
              control.kind === "config" && pendingChange?.option_id === control.option.id)}
          />
          {openMenu === "options" ? (
            <Popover className="composer-overflow-menu" label="More options">
              {hiddenControls.map((control) => (
                <MenuButton
                  className="composer-overflow-menu-row"
                  description={controlDescription(control, pendingChange, selection)}
                  disabled={control.kind === "config" ? configLocked : controlsLocked}
                  endIcon={<ChevronRight size={12} />}
                  icon={controlIcon(control, 13)}
                  key={controlKey(control)}
                  label={controlLabel(control)}
                  onClick={() => setOpenMenu(menuForControl(control))}
                />
              ))}
            </Popover>
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
  onSelectConfigOption?: (configId: string, value: string) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  openMenu?: ComposerMenu;
  pendingChange?: NonNullable<ConfigOptionsCatalog["pending_change"]>;
  selectAndClose: (select: () => void) => void;
  selection: ComposerSelection;
  toggleMenu: (menu: ComposerMenu) => void;
}) {
  const menu = menuForControl(control);
  const locked = control.kind === "config" ? configLocked : controlsLocked;
  const pending = control.kind === "config" && pendingChange?.option_id === control.option.id;
  return (
    <div className={`composer-option-anchor ${control.kind === "config" ? "composer-config-control-anchor" : "composer-isolation-control-anchor"}`}>
      <Selector
        className={control.kind === "config" ? "composer-config-control" : "composer-isolation-control"}
        disabled={locked}
        icon={controlIcon(control)}
        label={controlDirectLabel(control, pending ? pendingChange?.requested_value : undefined, selection)}
        locked={locked}
        menuOpen={openMenu === menu}
        onClick={() => toggleMenu(menu)}
        pending={pending}
      />
      {openMenu === menu ? (
        <ControlValueMenu
          configLocked={configLocked}
          control={control}
          controlsLocked={controlsLocked}
          onSelectConfigOption={(optionId, value) =>
            selectAndClose(() => onSelectConfigOption?.(optionId, value))}
          onSelectIsolation={(isolation) =>
            selectAndClose(() => onSelectIsolation?.(isolation))}
          selection={selection}
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
  onSelectConfigOption?: (configId: string, value: string) => void;
  onSelectIsolation?: (isolation: IsolationKind) => void;
  selection: ComposerSelection;
  setOpenMenu: Dispatch<SetStateAction<ComposerMenu | undefined>>;
}) {
  return (
    <ControlValueMenu
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

function ControlValueMenu({
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
  onSelectConfigOption: (configId: string, value: string) => void;
  onSelectIsolation: (isolation: IsolationKind) => void;
  selection: ComposerSelection;
}) {
  const label = controlLabel(control);
  return (
    <Popover className="composer-model-menu" label={label}>
      {onBack ? (
        <PopoverBackButton ariaLabel="Back to options" icon={<ChevronLeft size={13} />} label={label} onClick={onBack} />
      ) : null}
      {control.kind === "config" ? control.option.values.map((value) => (
        <MenuButton
          active={control.option.current_value === value.id}
          description={value.description ?? value.group_label ?? control.option.description ?? ""}
          disabled={configLocked}
          icon={configIcon(control.option, 13)}
          key={value.id}
          label={value.label}
          onClick={() => onSelectConfigOption(control.option.id, value.id)}
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
    </Popover>
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
          <div className="composer-option-anchor" key={controlKey(control)} ref={(node) => setOptionMeasureRef(index, node)}>
            <Selector
              disabled={locked}
              icon={controlIcon(control)}
              label={controlDirectLabel(control, pending ? pendingChange?.requested_value : undefined, selection)}
              locked={locked}
              menuOpen={false}
              onClick={() => {}}
              pending={pending}
            />
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

function menuForControl(control: RunControl): ComposerRunMenu {
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
  const selected = control.option.values.find((value) => value.id === displayedValue);
  return `Current: ${normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(displayedValue) ?? displayedValue}`;
}

function controlDirectLabel(control: RunControl, displayedValue: string | undefined, selection: ComposerSelection) {
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
  const selected = option.values.find((value) => value.id === displayedValue);
  const valueLabel = normalizedConfigValueLabel(selected?.label) ?? humanizeConfigValue(displayedValue) ?? option.label;
  const prefix = configOptionPrefix(option);
  return prefix ? `${prefix}: ${valueLabel}` : valueLabel;
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

function configOptionPrefix(option: ConfigOption) {
  if (option.category === "model") return undefined;
  const label = option.label.trim();
  if (!label || option.category === "mode" || option.category === "thought_level") return undefined;
  return label.replace(/\s+mode$/i, "");
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
