import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri serves the built files from disk; no dev server is used at run time.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  base: "./",
  build: { target: "es2022", outDir: "dist", sourcemap: false },
  server: { port: 1420, strictPort: true },
  // css: the palette test reads styles.css as text (?raw); without it Vitest stubs every CSS import empty.
  test: { environment: "node", include: ["src/**/*.test.ts"], css: true },
});
