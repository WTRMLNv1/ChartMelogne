import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  resolve: {
    alias: {
      chartmln: path.resolve(__dirname, '../web/src/index.ts'),
    },
  },
  server: {
    port: 5173,
    open: false,
  },
});
