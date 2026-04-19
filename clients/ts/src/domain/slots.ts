import type { RequestTransport } from "../transport/request.js";
import { WriteSlotResponseSchema } from "../schemas/slot.js";

/**
 * Slot writes against `POST /api/v1/slots` `{path, slot, value}`.
 *
 * A standalone slot read endpoint isn't shipped — the full node
 * snapshot from `nodes.getNode(path)` already carries every slot's
 * current value and generation. Left out here so the surface stays
 * one-op and easy to test.
 */

const WriteSlotRespSchema = WriteSlotResponseSchema;
type WriteSlotResp = { generation: number };

export interface SlotsApi {
  /** Returns the new generation number assigned by the graph store. */
  writeSlot(path: string, slot: string, value: unknown): Promise<number>;
}

export function createSlotsApi(http: RequestTransport, apiVersion: number): SlotsApi {
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
