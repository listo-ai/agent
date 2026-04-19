/** Semver of this client package — follows the package.json "version". */
export const CLIENT_VERSION = "0.1.0" as const;

/**
 * REST API version this client targets.
 * The client calls /api/v${REST_API_VERSION}/... for all endpoints.
 * Bumping this const is a major bump of the client package.
 */
export const REST_API_VERSION = 1 as const;

export interface CapabilityRequirement {
  id: string;
  /** Semver range the host-provided capability must satisfy. */
  range: string;
}

/**
 * Capabilities the host agent must provide before any call proceeds.
 * Mirrors the `requires!` macro pattern in the Rust extension system.
 */
export const REQUIRED_CAPABILITIES: readonly CapabilityRequirement[] = [
  { id: "spi.msg",         range: "^1" },
  { id: "spi.node.schema", range: "^1" },
] as const;
