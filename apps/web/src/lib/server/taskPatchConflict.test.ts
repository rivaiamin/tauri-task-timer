// Route-level regression test for the PATCH conflict path.
//
// The service-level test proves `updateTask` throws `LabelConflictError`; this
// one proves the route turns that into a 409 rather than letting it escape as a
// 500, which is what a caller actually sees. It invokes the real route handler
// with a stubbed actor, so the Zod schema and the error mapping are both real.
import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import { eq } from 'drizzle-orm';

const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-patch-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));

// JIRA is stubbed: a label like "US-1" parses as an issue key and loadCreds()
// would otherwise read the developer's real ~/.aimsis/jira.env.
vi.mock('$lib/server/jira', () => ({
	onCreate: vi.fn(async () => null),
	onStart: vi.fn(async () => {}),
	onStop: vi.fn(async () => {}),
	onSwitchStop: vi.fn(async () => {}),
	onDone: vi.fn(async () => {}),
	descriptionForTask: vi.fn(async (_l: string, d: string | null) => d)
}));

// The route resolves its actor from the session; hand it one directly so the
// test needs no cookie or API key.
vi.mock('$lib/server/actor', () => ({
	resolveActor: async () => ({ userId: USER, source: 'session', scopes: ['tasks:read', 'tasks:write'] }),
	requireScope: () => {}
}));

const USER = 'u-patch';
const DAY = '2026-09-21';

let svc: typeof import('./taskService');
let dbmod: typeof import('./db');
let PATCH: typeof import('../../routes/api/tasks/[id]/+server').PATCH;

/** The minimum RequestEvent surface the handler reads. */
function event(id: string, body: unknown) {
	return {
		params: { id },
		request: new Request('http://localhost/api/tasks/' + id, {
			method: 'PATCH',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(body)
		}),
		url: new URL('http://localhost/api/tasks/' + id)
	} as never;
}

/** Run the handler and normalise how SvelteKit reports an error. */
async function patch(id: string, body: unknown): Promise<{ status: number; message: string }> {
	try {
		const res = (await PATCH(event(id, body))) as Response;
		return { status: res.status, message: '' };
	} catch (err) {
		// SvelteKit's `error()` throws a HttpError carrying `status` and `body`.
		const e = err as { status?: number; body?: { message?: string }; message?: string };
		return { status: e.status ?? 500, message: e.body?.message ?? e.message ?? '' };
	}
}

beforeAll(async () => {
	svc = await import('./taskService');
	dbmod = await import('./db');
	PATCH = (await import('../../routes/api/tasks/[id]/+server')).PATCH;
	dbmod.db
		.insert(dbmod.schema.users)
		.values({ id: USER, email: 'patch@test.com', passwordHash: 'x', createdAt: new Date() })
		.run();
});

afterAll(() => {
	rmSync(dir, { recursive: true, force: true });
});

describe('PATCH /api/tasks/:id', () => {
	it('answers 409, not 500, when a rename collides with an existing label', async () => {
		await svc.createTask(USER, { label: 'TAKEN-1', description: null, workDate: DAY });
		const b = await svc.createTask(USER, { label: 'MINE-1', description: null, workDate: DAY });

		const res = await patch(String(b.id), { label: 'TAKEN-1' });
		expect(res.status).toBe(409);
		expect(res.message).toContain('TAKEN-1');

		// The refused rename changed nothing.
		expect(
			dbmod.db.select().from(dbmod.schema.tasks).where(
				eq(dbmod.schema.tasks.id, b.id)
			).get()!.label
		).toBe('MINE-1');
	});

	it('still applies an ordinary update', async () => {
		const t = await svc.createTask(USER, { label: 'PLAIN-1', description: null, workDate: DAY });
		const res = await patch(String(t.id), { label: 'PLAIN-2' });
		expect(res.status).toBe(200);
	});

	it('answers 404 for a task that is not the caller\u2019s', async () => {
		const res = await patch('999999', { label: 'NOPE-1' });
		expect(res.status).toBe(404);
	});
});

