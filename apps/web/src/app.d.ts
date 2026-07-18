// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
import type { SessionUser, SessionInfo } from '$lib/server/auth/session';

declare global {
	namespace App {
		// interface Error {}
		interface Locals {
			user: SessionUser | null;
			session: SessionInfo | null;
		}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
