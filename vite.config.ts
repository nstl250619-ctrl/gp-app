import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri v2 固定 dev 端口 1420，端口被占用时直接失败（strictPort），避免窗口连错。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      // '@' 指向 src（与 tsconfig paths 对齐；vite 不读 tsconfig，必须在此声明）
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Windows：Rust 构建时 target 下的 .exe 会被占用/替换，Vite 监听它会 EBUSY 崩溃。
      // 忽略整个 src-tauri，Vite 无需监听 Rust 源码。
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'es2021',
    outDir: 'dist',
  },
});
