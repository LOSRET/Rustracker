import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import ui from "@nuxt/ui/vite";

const contact = process.env.VITE_PERSONAL_CONTACT === "true"
  ? { blogUrl: "https://blog.7471.top/", email: "tracker@mail.7471.top" }
  : null;

const analytics = process.env.VITE_PERSONAL_CONTACT === "true"
  ? { src: "https://u.7471.top/script.js", id: "dabdcda9-0b8c-4cc6-8d16-d99ba68462cb" }
  : null;

export default defineConfig({
  plugins: [
    vue(),
    ui({
      router: false,
      ui: {
        colors: {
          primary: "blue",
          neutral: "slate",
        },
      },
    }),
    analytics && {
      name: "inject-analytics",
      transformIndexHtml(html: string) {
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
          if (id.includes("node_modules/echarts") || id.includes("node_modules/vue-echarts")) return "echarts";
          if (id.includes("node_modules/vue/")) return "vue";
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
