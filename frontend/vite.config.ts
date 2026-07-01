import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

const contact = process.env.VITE_PERSONAL_CONTACT === "true"
  ? { blogUrl: "https://blog.7471.top/", email: "tracker@mail.7471.top" }
  : null;

export default defineConfig({
  plugins: [vue()],
  define: {
    __CONTACT__: JSON.stringify(contact),
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/vue")) return "vue";
          if (id.includes("node_modules/echarts")) return "echarts";
        },
      },
    },
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/announce": "http://127.0.0.1:8080",
      "/scrape": "http://127.0.0.1:8080",
    },
  },
});
