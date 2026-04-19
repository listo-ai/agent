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
