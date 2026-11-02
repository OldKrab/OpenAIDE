import { ChevronLeft, ChevronRight } from "lucide-react";
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";

export type MobileTaskMoreSlot = "agents" | "messages" | "permission" | "terminal";
type Submenu = Exclude<MobileTaskMoreSlot, "permission" | "terminal">;

type MobileTaskMoreState = {
  motion?: "back" | "forward";
  open: boolean;
  submenu?: Submenu;
};

type MobileTaskMoreControls = MobileTaskMoreState & {
  close(): void;
  closeSubmenu(): void;
  openSubmenu(submenu: Submenu): void;
  toggle(): void;
};

const MobileTaskMoreContext = createContext<MobileTaskMoreControls>({
  close: () => undefined,
  closeSubmenu: () => undefined,
  open: false,
  openSubmenu: () => undefined,
  toggle: () => undefined,
});

/**
 * Ephemeral state of the phone's More menu. It sits above the Task bar and Task View because the
 * bar owns the menu while TaskView portals most of its rows in, and a portal keeps its React parent.
 */
export function MobileTaskMoreProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<MobileTaskMoreState>({ open: false });
  const actions = useMemo(() => ({
    close: () => setState({ open: false }),
    closeSubmenu: () => setState((current) => (
      current.submenu ? { motion: "back", open: current.open } : current
    )),
    openSubmenu: (submenu: Submenu) => setState({ motion: "forward", open: true, submenu }),
    toggle: () => setState((current) => ({ open: !current.open })),
  }), []);
  const value = useMemo(() => ({ ...state, ...actions }), [actions, state]);
  return <MobileTaskMoreContext.Provider value={value}>{children}</MobileTaskMoreContext.Provider>;
}

export const useMobileTaskMore = () => useContext(MobileTaskMoreContext);

/** One More menu row. Every row gives way while a submenu fills the menu. */
export function MobileTaskMoreRow({
  buttonRef,
  end,
  icon,
  label,
  slot,
  value,
  ...button
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className" | "type" | "value"> & {
  buttonRef?: Ref<HTMLButtonElement>;
  end?: ReactNode;
  icon: ReactNode;
  label: string;
  /** Rows mount in any order; the slot keeps each in its place. */
  slot: MobileTaskMoreSlot;
  value?: ReactNode;
}) {
  const { submenu } = useMobileTaskMore();
  if (submenu) return null;
  return (
    <button {...button} className="mobile-task-more-row" data-slot={slot} ref={buttonRef} type="button">
      <span className="mobile-task-more-icon">{icon}</span>
      <span className="mobile-task-more-copy">
        <strong>{label}</strong>
        {value ? <small>{value}</small> : null}
      </span>
      {end ? <span className="mobile-task-more-end">{end}</span> : null}
    </button>
  );
}

/**
 * A row that replaces the menu's content with `children`, so nothing stacks on top of the menu.
 * The menu is the one scroller and the heading stays pinned in it.
 */
export function MobileTaskMoreSubmenu({
  children,
  end,
  label,
  meta,
  slot,
  ...row
}: Omit<Parameters<typeof MobileTaskMoreRow>[0], "buttonRef" | "onClick" | "slot"> & {
  children: ReactNode;
  meta?: ReactNode;
  slot: Submenu;
}) {
  const menu = useMobileTaskMore();
  const active = menu.submenu === slot;
  const backRef = useRef<HTMLButtonElement>(null);
  const rowRef = useRef<HTMLButtonElement>(null);
  const returning = useRef(false);
  const { closeSubmenu, open } = menu;
  useEffect(() => {
    if (active) backRef.current?.focus();
    else if (returning.current && open) rowRef.current?.focus();
    returning.current = active;
  }, [active, open]);
  // A submenu whose control went away must not leave the menu empty.
  useEffect(() => (active ? closeSubmenu : undefined), [active, closeSubmenu]);
  if (active) {
    return (
      <section aria-label={label} className="mobile-task-submenu" role="group">
        <div className="mobile-task-submenu-heading">
          <button aria-label="Back to Task actions" onClick={menu.closeSubmenu} ref={backRef} type="button">
            <ChevronLeft aria-hidden="true" />
          </button>
          <strong>{label}</strong>
          {meta ? <small>{meta}</small> : null}
        </div>
        {children}
      </section>
    );
  }
  return (
    <MobileTaskMoreRow
      {...row}
      buttonRef={rowRef}
      end={<>{end}<ChevronRight aria-hidden="true" className="mobile-task-more-chevron" /></>}
      label={label}
      onClick={() => menu.openSubmenu(slot)}
      slot={slot}
    />
  );
}
