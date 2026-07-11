import type { ReactNode, RefObject } from "react";

export function SidebarRowActionSlot({ children, containerRef }: { children: ReactNode; containerRef?: RefObject<HTMLDivElement | null> }) {
  return (
    <div className="task-row-action-slot" ref={containerRef}>
      {children}
    </div>
  );
}
