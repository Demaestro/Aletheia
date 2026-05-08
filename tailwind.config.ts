import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // CSS-variable-backed so [data-theme="light"] / [data-theme="dark"]
        // overrides in styles/index.css can repaint the whole UI without
        // touching every component className.
        ink: "var(--c-ink)",
        graphite: "var(--c-graphite)",
        muted: "var(--c-muted)",
        line: "var(--c-line)",
        mist: "var(--c-mist)",
        paper: "var(--c-paper)",
        accent: "var(--c-accent)",
        caution: "var(--c-caution)",
        danger: "var(--c-danger)"
      },
      fontFamily: {
        sans: ["Outfit", "Inter", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "SFMono-Regular", "monospace"]
      },
      boxShadow: {
        output: "0 24px 60px rgba(0, 0, 0, 0.4)",
        neon: "0 0 15px var(--tw-shadow-color), inset 0 0 5px var(--tw-shadow-color)",
        glass: "0 8px 32px 0 rgba(0, 0, 0, 0.37)"
      }
    }
  },
  plugins: []
} satisfies Config;
