/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{vue,ts}"],
  darkMode: "media",
  theme: {
    extend: {
      colors: {
        ink: "var(--color-ink)",
        muted: "var(--color-muted)",
        line: "var(--color-line)",
        panel: "var(--color-panel)",
        soft: "var(--color-soft)",
        side: "var(--color-side)",
        "side-active": "var(--color-side-active)",
        "side-hover": "var(--color-side-hover)",
        "side-sel": "var(--color-side-sel)",
        "side-border": "var(--color-side-border)",
        accent: "var(--color-accent)",
        "accent-dark": "var(--color-accent-dark)",
        "hover-soft": "var(--color-hover-soft)",
        "row-hover": "var(--color-row-hover)",
        "td-border": "var(--color-td-border)",
        "code-bg": "var(--color-code-bg)",
        good: "var(--color-good)",
        warn: "var(--color-warn)",
        violet: "var(--color-violet)",
        bad: "var(--color-bad)",
        peers: "var(--color-peers)",
      },
      fontFamily: {
        sans: ['Inter', '"Segoe UI"', 'Arial', 'sans-serif'],
        display: ['Sora', 'sans-serif'],
        mono: ['"SFMono-Regular"', 'Consolas', 'monospace'],
      },
      animation: {
        "rps-pulse": "rps-pulse 2s ease-in-out infinite",
      },
    },
  },
  plugins: [],
};
