import { type Config } from "tailwindcss";

const config: Config = {
    darkMode: "class",
    content: [
      "./index.html",
      "./src/**/*.{ts,tsx,js,jsx}",
      "./app/**/*.{ts,tsx,js,jsx}",    // ← include your Next.js /app folder
      "./pages/**/*.{ts,tsx,js,jsx}",  // ← if you’re using /pages
    ],
    theme: {
      extend: {
        colors: {
          primary: {
            DEFAULT: "#0D1B2A",    
            foreground: "#F8F9FA", // now you get `.text-primary-foreground`
          },
          secondary: {
            DEFAULT: "#415A77",
            foreground: "#F8F9FA",
          },
          accent: {
            DEFAULT: "#00A8E8",
            foreground: "#212529",
          },
          background: {
            DEFAULT: "#F8F9FA",
            foreground: "#212529",
          },

        success: "#2ECC71",         // Verde éxito
        warning: "#F39C12",         // Ámbar advertencia
        error: "#C0392B",           // Rojo apagado
      },
      borderRadius: {
        lg: "0.75rem",
        xl: "1rem",
        "2xl": "1.5rem",
      },
      fontFamily: {
        sans: ["Inter", "sans-serif"],
      },
    },
  },
  plugins: [],
};

export default config;
