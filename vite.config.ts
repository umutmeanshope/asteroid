import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(({ command }) => ({
  plugins: [
    svelte({
      // In dev, component styles are injected by each component's JS rather
      // than served as separate virtual CSS modules: vite-plugin-svelte 7
      // occasionally loses a component's compiled CSS after a reload, and
      // Vite then serves the raw .svelte source as the stylesheet (whole
      // components lose their styling). Builds still extract a CSS file.
      emitCss: command === "build",
    }),
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2022",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
}));
