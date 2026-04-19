# Frontend Architecture — Separation of Concerns

This document captures the conventions that keep presentation (the "look")
separate from business logic, making the UI easy to restyle or swap
component libraries long-term.

---

## Layer model

```
┌─────────────────────────────────────────────────┐
│  components/ui/   — Shadcn primitives           │  ← Zero logic
│  (Button, Badge, Card, Dialog, Table, …)        │     Pure look & feel
├─────────────────────────────────────────────────┤
│  components/      — Composite UI components     │  ← Minimal logic
│  (AddChildNodeDialog, NodeContextMenu, layout/) │     Compose primitives
├─────────────────────────────────────────────────┤
│  pages/           — Page shells                 │  ← Thin glue
│  (FlowsListPage, PluginsPage, …)               │     Connect hooks → views
├─────────────────────────────────────────────────┤
│  hooks/ + pages/*/use*.ts                       │  ← All business logic
│  (useFlowsList, useAgent, usePlugins, …)        │     Returns plain data + callbacks
├─────────────────────────────────────────────────┤
│  store/           — Global state (Zustand)      │  ← Domain state
│  (graph-store, ui, auth, flow, extensions)      │
├─────────────────────────────────────────────────┤
│  lib/             — Utilities & API clients     │  ← Zero React
│  (agent, utils, node-settings)                  │
└─────────────────────────────────────────────────┘
```

## Rules of thumb

1. **UI primitives** (`components/ui/`) come from shadcn/ui. They are
   generated via `pnpm dlx shadcn@latest add <component>` and live in
   individual files. The barrel file (`components/ui/index.ts`) re-exports
   everything — always import from `@/components/ui`.

2. **No raw `<button>` / `<input>` / `<table>` in pages or feature
   components.** Always reach for the shadcn primitive (`Button`, `Input`,
   `Table`, etc.) so the visual language stays consistent and a future
   theme change propagates automatically.

3. **Hooks own the logic, components own the pixels.** A page component
   should:
   - Call one or two hooks (`useFlowsList()`, `usePlugins()`, …)
   - Destructure the returned "view-model" object
   - Pass plain data + callbacks down to presentational sub-components
   
   This means you can unit-test hooks without rendering, and swap the
   entire visual layer without touching state logic.

4. **Co-locate page-specific hooks** next to the page file:
   ```
   pages/flows/FlowsListPage.tsx    ← thin shell
   pages/flows/useFlowsList.ts      ← all logic for that page
   ```

5. **Design tokens** live in `main.css` as CSS custom properties using the
   shadcn HSL convention (`--primary`, `--card`, etc.). The dark theme
   variant overrides those in the `.dark` class. Changing the colour
   palette means editing ~20 lines of CSS — no component code changes.

6. **Don't mix Tailwind utilities for structure with those for colour /
   theme.** Structural classes (`flex`, `grid`, `p-6`) are fine inline.
   Colour and decoration should reference design tokens
   (`text-muted-foreground`, `bg-card`, `border-border`) so they respond
   to the theme automatically.

## Adding a new shadcn component

```bash
cd frontend
pnpm dlx shadcn@latest add <name>      # e.g. "accordion"
```

This creates `src/components/ui/<name>.tsx`. Then add the re-export to
`src/components/ui/index.ts`.

## Installed primitives

| Component       | File                          |
|-----------------|-------------------------------|
| Badge           | `components/ui/badge.tsx`     |
| Button          | `components/ui/button.tsx`    |
| Card            | `components/ui/card.tsx`      |
| Context Menu    | `components/ui/context-menu.tsx` |
| Dialog          | `components/ui/dialog.tsx`    |
| Dropdown Menu   | `components/ui/dropdown-menu.tsx` |
| Input           | `components/ui/input.tsx`     |
| Label           | `components/ui/label.tsx`     |
| Scroll Area     | `components/ui/scroll-area.tsx` |
| Select          | `components/ui/select.tsx`    |
| Separator       | `components/ui/separator.tsx` |
| Sheet           | `components/ui/sheet.tsx`     |
| Skeleton        | `components/ui/skeleton.tsx`  |
| Table           | `components/ui/table.tsx`     |
| Tabs            | `components/ui/tabs.tsx`      |
| Tooltip         | `components/ui/tooltip.tsx`   |
