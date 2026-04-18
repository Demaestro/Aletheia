import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [
    react({
      babel: {
        plugins: [["babel-plugin-react-compiler", {}]],
      },
    }),
  ],
  server: {
    host: "127.0.0.1",
    port: 5178
  },
  build: {
    // Single vendor chunk avoids circular-init runtime bugs.
    chunkSizeWarningLimit: 900
  }
});
