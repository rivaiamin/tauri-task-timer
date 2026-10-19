import { writable } from 'svelte/store';

/** In-flight async operations (API calls). Dashboard layout shows a global bar when > 0. */
export const pendingCount = writable(0);

export async function withPending<T>(work: () => Promise<T>): Promise<T> {
	pendingCount.update((n) => n + 1);
	try {
		return await work();
	} finally {
		pendingCount.update((n) => Math.max(0, n - 1));
	}
}
