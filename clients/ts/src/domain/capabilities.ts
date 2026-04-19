import type { HttpClient } from "../transport/http.js";
import { CapabilityManifestSchema } from "../schemas/capability.js";
import type { CapabilityManifest } from "../schemas/capability.js";
import type { CapabilityMismatchError } from "../errors.js";
import { REQUIRED_CAPABILITIES, REST_API_VERSION } from "../version.js";
import { satisfies, coerce } from "semver";

export interface CapabilitiesApi {
  /** Fetch the host's capability manifest. */
  getManifest(): Promise<CapabilityManifest>;
  /**
   * Assert every required capability is satisfied by the manifest.
   * Throws a CapabilityMismatchError if not.
   */
  assertRequirements(manifest: CapabilityManifest): void;
}

export function createCapabilitiesApi(http: HttpClient): CapabilitiesApi {
  return {
    async getManifest(): Promise<CapabilityManifest> {
      const raw = await http.get<unknown>(`/api/v${REST_API_VERSION}/capabilities`);
      return CapabilityManifestSchema.parse(raw);
    },

    assertRequirements(manifest: CapabilityManifest): void {
      const missing: CapabilityMismatchError["missing"] = [];

      for (const req of REQUIRED_CAPABILITIES) {
        const found = manifest.capabilities.find((c) => c.id === req.id);
        const version = found ? coerce(found.version) : null;
        const satisfied = version ? satisfies(version, req.range) : false;
        if (!satisfied) {
          missing.push({
            id: req.id,
            required: req.range,
            ...(found?.version !== undefined && { found: found.version }),
          });
        }
      }

      if (missing.length > 0) {
        const err: CapabilityMismatchError = { kind: "CapabilityMismatch", missing };
        throw err;
      }
    },
  };
}
