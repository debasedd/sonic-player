/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Surfaces are intentionally translucent: the window itself is
        // transparent and the acrylic backdrop does the heavy lifting.
        glass: {
          DEFAULT: "rgba(20, 20, 24, 0.68)",
          deep: "rgba(12, 12, 16, 0.82)",
          raised: "rgba(255, 255, 255, 0.06)",
          hover: "rgba(255, 255, 255, 0.12)",
        },
        line: {
          DEFAULT: "rgba(255, 255, 255, 0.13)",
          soft: "rgba(255, 255, 255, 0.07)",
        },
        ink: {
          DEFAULT: "#FFFFFF",
          muted: "#A1A1AA",
          faint: "#6E6E78",
        },
        spotify: {
          DEFAULT: "#1ED760",
          dim: "#1DB954",
        },
      },
      fontFamily: {
        sans: [
          "Inter var",
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI Variable Display",
          "Segoe UI",
          "system-ui",
          "sans-serif",
        ],
      },
      fontSize: {
        "2xs": ["10px", { lineHeight: "13px" }],
      },
      borderRadius: {
        pill: "28px",
        card: "22px",
        art: "14px",
      },
      boxShadow: {
        glass:
          "0 24px 60px -12px rgba(0,0,0,0.75), 0 8px 24px -8px rgba(0,0,0,0.6), inset 0 1px 0 0 rgba(255,255,255,0.14), inset 0 -1px 0 0 rgba(0,0,0,0.4)",
        "glass-sm":
          "0 10px 28px -10px rgba(0,0,0,0.7), inset 0 1px 0 0 rgba(255,255,255,0.11)",
        art: "0 8px 20px -6px rgba(0,0,0,0.8), 0 2px 6px rgba(0,0,0,0.5)",
        "glow-green": "0 0 0 1px rgba(30,215,96,0.35), 0 6px 20px -6px rgba(30,215,96,0.5)",
      },
      transitionTimingFunction: {
        glass: "cubic-bezier(0.32, 0.72, 0, 1)",
        spring: "cubic-bezier(0.34, 1.56, 0.64, 1)",
      },
      keyframes: {
        "fade-up": {
          from: { opacity: "0", transform: "translateY(6px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "sheen": {
          "0%": { transform: "translateX(-120%)" },
          "100%": { transform: "translateX(220%)" },
        },
      },
      animation: {
        "fade-up": "fade-up 320ms cubic-bezier(0.32,0.72,0,1) both",
      },
    },
  },
  plugins: [],
};