import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(__dirname, '../..');

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    fs: {
      // Allow serving files from workspace root (for monorepo node_modules)
      allow: [workspaceRoot]
    }
  },
  resolve: {
    alias: {
      // Ensure proper resolution in monorepo
      $lib: path.resolve(__dirname, './src/lib')
    }
  }
});
