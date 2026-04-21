/**
 * Returns a stable callback that writes a single named slot for any node.
 *
 * Usage:
 *   const saveSettings  = useSlotWriter("settings");
 *   const saveTags      = useSlotWriter("config.tags");
 *   const saveAppearance = useSlotWriter("config.appearance");
 *
 * The callback signature `(path, value) => Promise<void>` matches what
 * `useSlotEditor` (and all domain slot-hooks) expect as their `onSave` prop.
 */
import { useCallback } from "react";
import { useAgent } from "@listo/ui-core";

export function useSlotWriter(slotName: string): (path: string, value: unknown) => Promise<void> {
  const agentQuery = useAgent();

  return useCallback(
    async (path: string, value: unknown) => {
      const agent = agentQuery.data;
      if (!agent) return;
      await agent.slots.writeSlot(path, slotName, value);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [agentQuery.data, slotName],
  );
}
