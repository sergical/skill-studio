// PROTO(home-calc): builds only the prototype page, with production React, into the scratch
// folder, so it can be timed and audited. Never part of the site build.
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

import base from "../../vite.config";

export default defineConfig((env) => {
  const config = base(env);
  return {
    ...config,
    root: fileURLToPath(new URL("../..", import.meta.url)),
    build: {
      ...config.build,
      outDir: fileURLToPath(new URL("../../../../.scratch/proto-home-calc/dist", import.meta.url)),
      emptyOutDir: true,
      rollupOptions: { input: { proto: fileURLToPath(new URL("./index.html", import.meta.url)) } },
    },
  };
});
