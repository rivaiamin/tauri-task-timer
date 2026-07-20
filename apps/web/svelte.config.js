import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	// Consult https://svelte.dev/docs/kit/integrations
	// for more information about preprocessors
	preprocess: vitePreprocess(),

	kit: {
		// Node adapter: required because the app talks to a local SQLite file via
		// the native better-sqlite3 module, so it needs a persistent Node server.
		adapter: adapter()
	}
};

export default config;
