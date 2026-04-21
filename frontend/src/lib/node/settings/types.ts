export type SaveState = "idle" | "saving" | "ok" | "error";

export interface NodeSettingsState {
  /** Current form values. */
  formData: Record<string, unknown>;
  /** Called by the form on every change; handles debounce + save. */
  onChange: (data: Record<string, unknown>) => void;
  saveState: SaveState;
  saveError: string | null;
}
