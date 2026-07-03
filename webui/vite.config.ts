import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // shadcn/ui 관례 별칭 — 컴포넌트 레지스트리 소스가 @/를 쓴다.
    alias: { '@': path.resolve(__dirname, 'src') },
  },
  server: {
    // Bind the IPv4 loopback explicitly. The default (`localhost`) resolves to
    // `::1` (IPv6) on macOS, so `127.0.0.1:5173` is refused — which the
    // claude-in-chrome extension (authorized on 127.0.0.1) needs. Loopback only,
    // not `host: true` (no LAN exposure — local-first).
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      // Default to the local serve on :7878. `WIMCC_PROXY_TARGET` lets a dev
      // point the preview at an isolated serve instance (e.g. a snapshot DB on
      // an alternate port) without restarting the serve the live Claude session
      // exports to — see memory `serve-restart-kills-live-claude`.
      '/v1': {
        target: process.env.WIMCC_PROXY_TARGET || 'http://127.0.0.1:7878',
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
