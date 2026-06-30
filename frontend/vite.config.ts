import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const contactEnabled = process.env.VITE_PERSONAL_CONTACT === "true";
let contactHtml = "";
if (contactEnabled) {
  const contactPath = path.resolve(__dirname, "../assets/contact.html");
  contactHtml = fs.readFileSync(contactPath, "utf-8");
}

export default defineConfig({
  plugins: [vue()],
  define: {
    __CONTACT_HTML__: JSON.stringify(contactHtml),
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/announce": "http://127.0.0.1:8080",
      "/scrape": "http://127.0.0.1:8080",
    },
  },
});
