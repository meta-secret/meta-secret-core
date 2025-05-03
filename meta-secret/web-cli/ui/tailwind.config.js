/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        gray: {
          750: '#2d3748',  // Between gray-700 and gray-800
          850: '#1a202c',  // Between gray-800 and gray-900
        }
      },
    },
  },
  plugins: [],
}
