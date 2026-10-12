/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        theme: {
          main: "var(--bg-main)",
          card: "var(--bg-card)",
          "card-hover": "var(--bg-card-hover)",
          text: "var(--text-main)",
          sub: "var(--text-sub)",
          "card-text": "var(--text-card)",
          "card-sub": "var(--text-card-sub)",
          border: "var(--border-main)",
          accent: "var(--accent-color)",
          cyan: "var(--accent-cyan)",
        },
      },
      fontFamily: {
        serif: ["var(--font-serif)", "Instrument Serif", "Georgia", "serif"],
        sans: ["var(--font-sans)", "Inter", "sans-serif"],
      },
      animation: {
        "meteor-effect": "meteor 5s linear infinite",
        "aurora": "aurora 60s linear infinite",
      },
      keyframes: {
        meteor: {
          "0%": { transform: "rotate(215deg) translateX(0)", opacity: 1 },
          "70%": { opacity: 1 },
          "100%": {
            transform: "rotate(215deg) translateX(-500px)",
            opacity: 0,
          },
        },
        aurora: {
          from: { backgroundPosition: "50% 50%, 50% 50%" },
          to: { backgroundPosition: "350% 50%, 350% 50%" },
        },
      },
    },
  },
  plugins: [],
};
