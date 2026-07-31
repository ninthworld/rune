import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  build: { outDir: 'dist', sourcemap: true },
  // `src/decks.ts` reads the shared `data/starter-decks.json` at the repository root, which is
  // outside this package. It is owned by neither side and must not be duplicated here.
  server: { fs: { allow: ['../..'] } },
})
