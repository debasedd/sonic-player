import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and ignores VITE_* variables in production builds.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5183,
    // Fail loudly instead of silently starting on another port: Tauri's devUrl
    // points at 5183, so a silent fallback would load a stale or empty page.
    strictPort: true,
    watch: {
      // src-tauri is watched by the Rust side; do not double-watch it here.
      ignored: ["**/src-tauri/**"],
    },
  },
  // Tauri's API packages ship as native-ish ESM that Vite's dep pre-bundler
  // rewrites in a way that breaks the IPC bridge. Excluding them keeps the
  // import specifiers the Tauri runtime expects, which is what removes the
  // intermittent "file does not exist in the optimize deps directory" error.
  optimizeDeps: {
    exclude: ["@tauri-apps/api"],
  },
  build: {
    // Tauri uses the local WebView2 runtime (Chromium), so a modern target is safe.
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
});