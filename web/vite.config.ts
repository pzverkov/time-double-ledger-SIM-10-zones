import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(() => ({
  base: "./",
  plugins: [react()],
  server: {
    proxy: {
      "/v1": {
        target: process.env.VITE_PROXY_TARGET || "http://localhost:8080",
        changeOrigin: true
      },
      "/healthz": {
        target: process.env.VITE_PROXY_TARGET || "http://localhost:8080",
        changeOrigin: true
      },
      "/metrics": {
        target: process.env.VITE_PROXY_TARGET || "http://localhost:8080",
        changeOrigin: true
      }
    }
  }
}));
