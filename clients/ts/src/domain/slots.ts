import type { RequestTransport } from "../transport/request.js";
import { WriteSlotResponseSchema } from "../schemas/slot.js";
import { GenerationMismatchError } from "../errors.js";

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

export interface WriteSlotOptions {
  /** OCC guard — server 409s if slot's current generation differs. */
  expectedGeneration?: number;
}

export interface SlotsApi {
  /**
   * Returns the new generation number assigned by the graph store.
   * With `opts.expectedGeneration`, throws [`GenerationMismatchError`]
   * if the server's current generation differs.
   */
  writeSlot(
    path: string,
    slot: string,
    value: unknown,
    opts?: WriteSlotOptions,
  ): Promise<number>;
}

export function createSlotsApi(http: RequestTransport, apiVersion: number): SlotsApi {
  const base = `/api/v${apiVersion}`;
  return {
    async writeSlot(
      path: string,
      slot: string,
      value: unknown,
      opts?: WriteSlotOptions,
    ): Promise<number> {
      const body: Record<string, unknown> = { path, slot, value };
      if (opts?.expectedGeneration !== undefined) {
        body.expected_generation = opts.expectedGeneration;
      }
      try {
        const raw = await http.post<WriteSlotResp>(`${base}/slots`, body);
        return WriteSlotRespSchema.parse(raw).generation;
      } catch (err) {
        const current = parseGenerationMismatch(err);
        if (current !== null) throw new GenerationMismatchError(current);
        throw err;
      }
    },
  };
}

function parseGenerationMismatch(err: unknown): number | null {
  if (
    typeof err !== "object" ||
    err === null ||
    !("status" in err) ||
    (err as { status: unknown }).status !== 409 ||
    !("message" in err) ||
    typeof (err as { message: unknown }).message !== "string"
  ) {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse((err as { message: string }).message);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      (parsed as { code?: unknown }).code === "generation_mismatch" &&
      typeof (parsed as { current_generation?: unknown }).current_generation === "number"
    ) {
      return (parsed as { current_generation: number }).current_generation;
    }
  } catch {
    /* fall through */
  }
  return null;
}
