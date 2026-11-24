import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
const host = process.env.TAURI_DEV_HOST;
const isWindows = process.platform === "win32";

const windowsWebviewCompatibility: Plugin = {
  name: "tomatodesk-disable-vite-client",
  configureServer(server) {
    server.middlewares.use((request, response, next) => {
      if (request.url?.startsWith("/@vite/client")) {
        response.statusCode = 200;
        response.setHeader("Content-Type", "application/javascript");
        response.end(
          "export const createHotContext = () => ({ accept() {}, dispose() {}, prune() {}, on() {}, send() {}, invalidate() {}, decline() {} }); export const updateStyle = () => {}; export const removeStyle = () => {};",
        );
        return;
      }
      next();
    });
  },
  transformIndexHtml: {
    order: "post" as const,
    handler(html: string) {
      return html.replace(/<script type="module" src="\/@vite\/client"><\/script>\s*/g, "");
    },
  },
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    react(),
    tailwindcss(),
    // Tauri's Windows WebView does not need HMR; serve the CSS runtime imports as no-ops.
    ...(isWindows ? [windowsWebviewCompatibility] : []),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : isWindows
        ? false
        : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
