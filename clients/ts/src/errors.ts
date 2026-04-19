// Structured error discriminated union.
// No raw thrown strings anywhere in this package.

export interface HttpError {
  kind: "HttpError";
  status: number;
  message: string;
}

export interface CapabilityMismatchError {
  kind: "CapabilityMismatch";
  missing: Array<{
    id: string;
    required: string;
    found?: string;
  }>;
}

export interface ParseError {
  kind: "ParseError";
  message: string;
}

export interface TimeoutError {
  kind: "Timeout";
  message: string;
}

/**
 * Thrown by `slots.writeSlot` when the server 409s an OCC-guarded
 * write. Builder UI converts this to the conflict banner.
 */
export class GenerationMismatchError extends Error {
  readonly kind = "GenerationMismatch" as const;
  readonly status = 409 as const;
  constructor(public readonly currentGeneration: number) {
    super(`generation mismatch: current ${currentGeneration}`);
    this.name = "GenerationMismatchError";
  }
}

export type ClientError =
  | HttpError
  | CapabilityMismatchError
  | ParseError
  | TimeoutError;

/** Narrow a thrown unknown to ClientError or re-throw. */
export function asClientError(err: unknown): ClientError {
  if (
    typeof err === "object" &&
    err !== null &&
    "kind" in err &&
    typeof (err as { kind: unknown }).kind === "string"
  ) {
    return err as ClientError;
  }
  throw err;
}
