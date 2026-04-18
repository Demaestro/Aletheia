import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#F8FAFC",
        graphite: "#94A3B8",
        muted: "#64748B",
        line: "#1E293B",
        mist: "#0F172A",
        paper: "#020617",
        accent: "#8B5CF6",
        caution: "#F59E0B",
        danger: "#EF4444"
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
