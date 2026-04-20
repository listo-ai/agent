// Block manifest types — mirrors the manifest.json shape described in UI.md.
// These are the types the Studio uses; they are not generated from a schema yet.

export type TrustTier = "first-party" | "signed-vetted" | "untrusted";

export interface ExtensionNodeContribution {
  /** Node kind id, e.g. "bacnet.device". */
  id: string;
  /** Human label shown in the node palette. */
  label: string;
  /** Path to JSON Schema for property panel. */
  schema: string;
}

export interface ExtensionPanelContribution {
  /** Panel id. */
  id: string;
  /** Which node kind(s) this panel targets. */
  target: "node-config";
  nodeKinds: string[];
}

export interface ExtensionViewContribution {
  id: string;
  location: "sidebar" | "main";
  label: string;
}

export interface ExtensionWidgetContribution {
  id: string;
  label: string;
}

export interface ExtensionContributions {
  nodes?:    ExtensionNodeContribution[];
  panels?:   ExtensionPanelContribution[];
  views?:    ExtensionViewContribution[];
  widgets?:  ExtensionWidgetContribution[];
}

export interface BlockManifest {
  /** Unique dot-namespaced id, e.g. "sys.bacnet". */
  id: string;
  name: string;
  version: string;
  /** Trust tier determines load strategy (direct MF vs iframe sandbox). */
  trust: TrustTier;
  /** URL to the Module Federation remote entry (mf-manifest.json), for trusted blocks. */
  remoteEntry?: string;
  /** MF container name, e.g. "plugin_hello". Must match the remote's ModuleFederationPlugin `name`. */
  remoteName?: string;
  /** Exposed module path to load, e.g. "./Panel". Defaults to "./Panel". */
  exposedModule?: string;
  contributions: ExtensionContributions;
}
