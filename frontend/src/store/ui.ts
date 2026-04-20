import { create } from "zustand";
import { persist } from "zustand/middleware";

// UI state: sidebar collapse, active panel, theme.
// Kept separate from domain stores so layout decisions don't cross into flow state.

type Theme = "light" | "dark" | "system";
type SidebarSection = "flows" | "dashboard" | "blocks" | "settings";

interface UiState {
  sidebarCollapsed: boolean;
  activeSection: SidebarSection;
  theme: Theme;
  commandPaletteOpen: boolean;

  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  setActiveSection: (section: SidebarSection) => void;
  setTheme: (theme: Theme) => void;
  setCommandPaletteOpen: (open: boolean) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      activeSection: "flows",
      theme: "system",
      commandPaletteOpen: false,

      setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      setActiveSection: (activeSection) => set({ activeSection }),
      setTheme: (theme) => set({ theme }),
      setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
    }),
    {
      name: "us-ui",
      partialize: (state) => ({ theme: state.theme, sidebarCollapsed: state.sidebarCollapsed }),
    }
  )
);
