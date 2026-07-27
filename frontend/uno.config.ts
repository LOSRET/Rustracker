import { defineConfig, presetWind3, presetIcons } from "unocss"

// 颜色直接引用 style.css 中 :root 定义的 CSS 变量,暗色模式下变量值由
// @media (prefers-color-scheme: dark) 自动切换,无需在此处重复维护两套色值。
export default defineConfig({
  presets: [presetWind3({ dark: "media" }), presetIcons()],
  theme: {
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
      "side-fg": "var(--color-side-fg)",
      "side-muted": "var(--color-side-muted)",
      "side-note": "var(--color-side-note)",
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
      sans: '"Segoe UI", Arial, sans-serif',
      display: "Sora, sans-serif",
      mono: '"SFMono-Regular", Consolas, monospace',
    },
    animation: {
      "rps-pulse": "rps-pulse 2s ease-in-out infinite",
      "tooltip-in": "tooltip-in 160ms ease-out",
    },
    keyframes: {
      "tooltip-in": {
        from: { opacity: "0", scale: "0.92" },
        to: { opacity: "1", scale: "1" },
      },
    },
  },
  // Reka UI 的 Select / RadioGroup 等组件会输出 data-highlighted 属性
  // (无值布尔形式),需要手动注册变体以支持 data-highlighted:xxx 写法。
  variants: [
    (matcher) => {
      if (!matcher.startsWith("data-highlighted:")) return matcher
      return {
        matcher: matcher.slice("data-highlighted:".length),
        selector: (s) => `[data-highlighted]${s}`,
      }
    },
  ],
})
