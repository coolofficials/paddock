import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
export default defineConfig({
  resolve: {
    alias: {
      "@astryxdesign/theme-neutral/fonts.css": fileURLToPath(
        new URL("./src/fonts.css", import.meta.url),
      ),
    },
  },
  build: {
    assetsInlineLimit: 10000000,
    cssCodeSplit: false,
    rollupOptions: {
      onwarn(warning, handler) {
        if (warning.code !== "MODULE_LEVEL_DIRECTIVE") handler(warning);
      },
      output: {
        entryFileNames: "assets/app.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/app.[ext]",
        inlineDynamicImports: true,
      },
    },
  },
  server: { host: "127.0.0.1" },
});
