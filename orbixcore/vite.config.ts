import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      output: {
        // Split the rarely-changing vendor code (React, the markdown parser)
        // out of the app chunk so it caches across app updates and the main
        // chunk shrinks.
        manualChunks: {
          "vendor-react": ["react", "react-dom"],
          "vendor-markdown": ["marked"],
        },
      },
    },
  },
}));
