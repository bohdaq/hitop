import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  // Tauri dev server port
  // fs.allow: shared request helpers come from ../webapp/src/features.js
  server: { port: 1420, strictPort: true, fs: { allow: ['..'] } },
  // Tauri expects the build output here
  build: { outDir: 'dist' },
  // Prevent Vite from obscuring Rust errors
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
});
