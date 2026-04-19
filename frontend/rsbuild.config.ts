import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Module Federation shared singletons — every dep listed here must appear
// in both host and remote configs with the same version string and
// singleton: true.  Adding a dep here without singleton: true is a bug.
const MF_SHARED_SINGLETONS = {
  react: {
    singleton: true,
    requiredVersion: "^19.0.0",
    eager: true,
  },
  "react-dom": {
    singleton: true,
    requiredVersion: "^19.0.0",
    eager: true,
  },
  "react-router-dom": {
    singleton: true,
    requiredVersion: "^7.0.0",
  },
  zustand: {
    singleton: true,
    requiredVersion: "^5.0.0",
  },
  "@tanstack/react-query": {
    singleton: true,
    requiredVersion: "^5.40.0",
  },
} as const;

export default defineConfig(({ envMode }) => {
  const isTauri = envMode === "tauri";

  return {
    plugins: [pluginReact()],

    source: {
      entry: { index: "./src/index.tsx" },
    },

    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },

    output: {
      target: "web",
      distPath: { root: isTauri ? "dist-tauri" : "dist-web" },
    },

    html: {
      template: "./index.html",
    },

    server: {
      port: 3000,
    },

    tools: {
      postcss: {
        postcssOptions: {
          plugins: [require("@tailwindcss/postcss")],
        },
      },
      rspack: (config, { appendPlugins }) => {
        const { ModuleFederationPlugin } = require("@module-federation/enhanced/rspack");

        appendPlugins(
          new ModuleFederationPlugin({
            name: "studio_host",
            // The host exposes the service registry so in-tree remote packages
            // can import it without bundling their own copy.
            exposes: {
              "./registry": "./src/providers/registry.tsx",
            },
            shared: MF_SHARED_SINGLETONS,
          }),
        );

        if (isTauri) {
          config.devtool = "source-map";
        }
      },
    },
  };
});
