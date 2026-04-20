import { registerRemotes, loadRemote } from "@module-federation/enhanced/runtime";
import type { ExtensionManifest } from "./types";
import { registerExtensionContributions } from "./registry";
import { useExtensionsStore } from "@/store/extensions";

// Extension loader — uses @module-federation/enhanced's runtime API.
//
// Trust tiers (from UI.md):
//   first-party / signed-vetted  → registerRemotes + loadRemote into host realm
//   untrusted                    → iframe + postMessage bridge (Milestone 6+)

export async function fetchManifest(url: string): Promise<ExtensionManifest> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to fetch manifest from ${url}: ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<ExtensionManifest>;
}

interface LoadedModule {
  default?: unknown;
  [k: string]: unknown;
}

// Trusted path — MF runtime API. `remoteEntry` points at the remote's
// mf-manifest.json (or remoteEntry.js for classic MF).
async function loadTrustedExtension(manifest: ExtensionManifest): Promise<LoadedModule> {
  if (!manifest.remoteEntry) {
    throw new Error(`Extension "${manifest.id}" has no remoteEntry URL`);
  }
  if (!manifest.remoteName) {
    throw new Error(`Extension "${manifest.id}" has no remoteName (MF container name)`);
  }

  registerRemotes([{ name: manifest.remoteName, entry: manifest.remoteEntry }]);

  const exposed = manifest.exposedModule ?? "./Panel";
  const mod = await loadRemote<LoadedModule>(`${manifest.remoteName}/${exposed.replace(/^\.\//, "")}`);
  if (!mod) {
    throw new Error(`loadRemote returned null for ${manifest.remoteName}/${exposed}`);
  }
  return mod;
}

export async function loadExtension(manifest: ExtensionManifest): Promise<LoadedModule | null> {
  const { registerManifest, setStatus } = useExtensionsStore.getState();

  registerManifest(manifest);
  setStatus(manifest.id, "loading");

  try {
    if (manifest.trust === "untrusted") {
      console.warn(`[extensions] Untrusted extension "${manifest.id}" skipped — iframe sandbox not yet implemented`);
      setStatus(manifest.id, "idle");
      return null;
    }

    const mod = await loadTrustedExtension(manifest);
    registerExtensionContributions(manifest);
    setStatus(manifest.id, "loaded");
    return mod;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error(`[extensions] Failed to load "${manifest.id}":`, message);
    setStatus(manifest.id, "error", message);
    return null;
  }
}

export async function loadExtensions(manifestUrls: string[]): Promise<void> {
  await Promise.allSettled(
    manifestUrls.map(async (url) => {
      const manifest = await fetchManifest(url);
      await loadExtension(manifest);
    }),
  );
}
