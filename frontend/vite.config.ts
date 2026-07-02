import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

const contact = process.env.VITE_PERSONAL_CONTACT === "true"
  ? { blogUrl: "https://blog.7471.top/", email: "tracker@mail.7471.top" }
  : null;

const analytics = process.env.VITE_PERSONAL_CONTACT === "true"
  ? { src: "https://u.7471.top/script.js", id: "dabdcda9-0b8c-4cc6-8d16-d99ba68462cb" }
  : null;

export default defineConfig({
  plugins: [
    vue(),
    analytics && {
      name: "inject-analytics",
      transformIndexHtml(html) {
        const tag = `<script defer src="${analytics.src}" data-website-id="${analytics.id}"></script>`;
        return html.replace("</head>", `  ${tag}\n</head>`);
      },
    },
  ].filter(Boolean),
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
