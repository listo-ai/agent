import { createRequire } from "node:module";
import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";

const require = createRequire(import.meta.url);

// Must match the host's shared singletons exactly — same versions, same flags.
// This is the contract MF enforces: drift here causes duplicate copies.
const MF_SHARED_SINGLETONS = {
  react:                  { singleton: true, requiredVersion: "^19.0.0" },
  "react-dom":            { singleton: true, requiredVersion: "^19.0.0" },
  zustand:                { singleton: true, requiredVersion: "^5.0.0" },
  "@tanstack/react-query":{ singleton: true, requiredVersion: "^5.40.0" },
} as const;

export default defineConfig({
  plugins: [pluginReact()],

  source: {
    entry: { index: "./src/bootstrap.ts" },
  },

  output: {
    target: "web",
    distPath: { root: "dist" },
  },

  server: {
    port: 3001,
  },

  dev: {
    writeToDisk: true,
  },

  tools: {
    rspack: (_config, { appendPlugins }) => {
      const { ModuleFederationPlugin } = require("@module-federation/enhanced/rspack");

      appendPlugins(
        new ModuleFederationPlugin({
          name: "plugin_hello",
          filename: "remoteEntry.js",
          exposes: {
            "./Panel": "./src/Panel.tsx",
          },
          shared: MF_SHARED_SINGLETONS,
        }),
      );
    },
  },
});
