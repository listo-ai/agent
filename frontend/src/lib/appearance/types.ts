export interface NodeAppearance {
  /** Lucide icon name, e.g. "activity". */
  icon?: string;
  /** CSS color — hex (`#3b82f6`) or Tailwind token (`blue-500`). */
  color?: string;
}

export type AppearanceSaveState = "idle" | "saving" | "ok" | "error";

export interface NodeAppearanceState {
  appearance: NodeAppearance;
  saveState: AppearanceSaveState;
  saveError: string | null;
  setIcon: (icon: string) => void;
  setColor: (color: string) => void;
  clear: (field: "icon" | "color") => void;
}
