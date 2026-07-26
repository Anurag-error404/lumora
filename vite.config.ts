import { renameSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
const rootDir = fileURLToPath(new URL(".", import.meta.url));

/** Emit dist/index.html for Tauri while keeping root index.html as the Pages landing. */
function appHtmlAsIndex(): Plugin {
  return {
    name: "app-html-as-index",
    closeBundle() {
      const from = resolve(rootDir, "dist/app.html");
      const to = resolve(rootDir, "dist/index.html");
      if (existsSync(from)) {
        renameSync(from, to);
      }
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), appHtmlAsIndex()],

  build: {
    rollupOptions: {
      input: resolve(rootDir, "app.html"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
