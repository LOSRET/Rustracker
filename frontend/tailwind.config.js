/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{vue,ts}"],
  darkMode: "media",
  theme: {
    extend: {
      fontFamily: {
        sans: ["Inter", "system-ui", "Segoe UI", "Roboto", "sans-serif"],
        display: ["Sora", "system-ui", "sans-serif"],
      },
    },
  },
  plugins: [],
};
