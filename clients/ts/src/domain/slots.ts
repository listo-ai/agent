import type { HttpClient } from "../transport/http.js";
import { z } from "zod";

/**
 * Slot writes against `POST /api/v1/slots` `{path, slot, value}`.
 *
 * A standalone slot read endpoint isn't shipped — the full node
 * snapshot from `nodes.getNode(path)` already carries every slot's
 * current value and generation. Left out here so the surface stays
 * one-op and easy to test.
 */

const WriteSlotRespSchema = z.object({
  generation: z.number().int().nonnegative(),
});
type WriteSlotResp = z.infer<typeof WriteSlotRespSchema>;

export interface SlotsApi {
  /** Returns the new generation number assigned by the graph store. */
  writeSlot(path: string, slot: string, value: unknown): Promise<number>;
}

export function createSlotsApi(http: HttpClient, apiVersion: number): SlotsApi {
  const base = `/api/v${apiVersion}`;
  return {
    async writeSlot(path: string, slot: string, value: unknown): Promise<number> {
      const raw = await http.post<WriteSlotResp>(`${base}/slots`, {
        path,
        slot,
        value,
      });
      return WriteSlotRespSchema.parse(raw).generation;
    },
  };
}
