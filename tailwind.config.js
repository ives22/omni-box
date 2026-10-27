/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Inter"', '"IBM Plex Sans"', '-apple-system', 'BlinkMacSystemFont', '"PingFang SC"', 'sans-serif'],
        mono: ['"JetBrains Mono"', '"SF Mono"', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
      },
      colors: {
        obsidian: {
          950: '#090C15',
          900: '#0E131F',
          850: '#131A29',
          800: '#182032',
          700: '#232D42',
          600: '#334155',
          500: '#475569',
        },
        accent: {
          DEFAULT: '#4F46E5',
          hover: '#4338CA',
          glow: 'rgba(79, 70, 229, 0.14)',
        }
      },
      boxShadow: {
        'launcher': '0 14px 36px -8px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.10), inset 0 1px 0 0 rgba(255, 255, 255, 0.08)',
        'launcher-light': '0 12px 32px -6px rgba(15, 23, 42, 0.12), 0 2px 6px -1px rgba(15, 23, 42, 0.05), inset 0 1px 0 0 rgba(255, 255, 255, 0.9)',
      }
    },
  },
  plugins: [],
}
