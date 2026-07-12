import type { ComponentType, ReactNode } from "react";

export type PrototypeVariant = {
  key: string;
  name: string;
  Component: ComponentType;
};

export type PrototypeDefinition = {
  title: string;
  question: string;
  variants: PrototypeVariant[];
  defaultVariant?: string;
};

/** Gives ignored prototype modules a typed contract without coupling them to the product bundle. */
export function definePrototype(definition: PrototypeDefinition) {
  return definition;
}

/** Provides a neutral full-viewport canvas while production components keep their real styling. */
export function PrototypeCanvas({ children, className }: { children: ReactNode; className?: string }) {
  return <main className={["prototype-canvas", className].filter(Boolean).join(" ")}>{children}</main>;
}
