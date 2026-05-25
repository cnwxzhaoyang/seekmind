import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(async () => ({
  plugins: [vue(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "0.0.0.0",
    watch: {
      ignored: ["**/src-tauri/**", "**/.docmind-dev/**", "**/.docmind-dev/**/*"],
    },
  },
}));
