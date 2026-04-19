import type { HttpClient } from "../transport/http.js";
import { SlotSchema } from "../schemas/node.js";
import type { Slot } from "../schemas/node.js";

export interface SlotsApi {
  readSlot(nodePath: string, slotName: string): Promise<Slot>;
  writeSlot(nodePath: string, slotName: string, value: unknown): Promise<void>;
}

export function createSlotsApi(http: HttpClient, apiVersion: number): SlotsApi {
  const base = `/api/v${apiVersion}/nodes`;

  return {
    async readSlot(nodePath: string, slotName: string): Promise<Slot> {
      const raw = await http.get<unknown>(
        `${base}/${encodeURIComponent(nodePath)}/slots/${encodeURIComponent(slotName)}`,
      );
      return SlotSchema.parse(raw);
    },

    async writeSlot(nodePath: string, slotName: string, value: unknown): Promise<void> {
      await http.put<void>(
        `${base}/${encodeURIComponent(nodePath)}/slots/${encodeURIComponent(slotName)}`,
        { value },
      );
    },
  };
}
