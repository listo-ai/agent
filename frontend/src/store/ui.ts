import { create } from "zustand";
import { persist } from "zustand/middleware";

// UI state: sidebar collapse, active panel, theme, nav mode.
// Kept separate from domain stores so layout decisions don't cross into flow state.

type Theme = "light" | "dark" | "system";
type SidebarSection = "flows" | "dashboard" | "blocks" | "settings";
export type NavMode = "admin" | "user";

interface UiState {
  activeSection: SidebarSection;
  theme: Theme;
  commandPaletteOpen: boolean;
  /** Which sidebar mode is active — admin Studio view or user nav-tree view. */
  navMode: NavMode;
  /** Root `ui.nav` node id shown in user mode. Null = unconfigured. */
  userNavRootId: string | null;

  setActiveSection: (section: SidebarSection) => void;
  setTheme: (theme: Theme) => void;
  setCommandPaletteOpen: (open: boolean) => void;
  setNavMode: (mode: NavMode) => void;
  setUserNavRootId: (id: string | null) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      activeSection: "flows",
      theme: "system",
      commandPaletteOpen: false,
      navMode: "admin",
      userNavRootId: null,

      setActiveSection: (activeSection) => set({ activeSection }),
      setTheme: (theme) => set({ theme }),
      setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
      setNavMode: (navMode) => set({ navMode }),
      setUserNavRootId: (userNavRootId) => set({ userNavRootId }),
    }),
    {
      name: "us-ui",
      partialize: (state) => ({ theme: state.theme, navMode: state.navMode, userNavRootId: state.userNavRootId }),
    }
  )
);
