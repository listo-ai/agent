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
        if (isGenerationMismatch(err)) {
          throw new GenerationMismatchError(err.current);
        }
        throw err;
      }
    },
  };
}

function isGenerationMismatch(
  err: unknown,
): err is { current: number } {
  if (
    typeof err !== "object" ||
    err === null ||
    !("status" in err) ||
    (err as { status: unknown }).status !== 409 ||
    !("message" in err)
  ) {
    return false;
  }
  try {
    const parsed = JSON.parse((err as { message: string }).message);
    if (
      parsed &&
      typeof parsed === "object" &&
      parsed.code === "generation_mismatch" &&
      typeof parsed.current_generation === "number"
    ) {
      (err as { current: number }).current = parsed.current_generation;
      return true;
    }
  } catch {
    /* fall through */
  }
  return false;
}
