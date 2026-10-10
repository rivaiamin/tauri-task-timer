// Integration test for the `task_integrations` contract `docs/api.md` documents
// and the sprint orchestrator depends on: upsert keyed by (task, group, field),
// `null` clearing a value without deleting the row, and ownership scoping.
//
// This drives the real service against a throwaway migrated DB, because the
// properties that matter (in-place replace, null-vs-delete, cross-user scoping)
// live in the SQL, not in a helper a unit test could stub.
//
// The env module is mocked for the same reason as taskService.status.test.ts:
// db/index.ts reads DATABASE_PATH from `$env/dynamic/private`, which Vite resolves
// from apps/web/.env and would otherwise point at the real database.
import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';

const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-integ-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));
// JIRA is stubbed so a label that parses as an issue key never reaches Atlassian.
vi.mock('./jira', () => ({
	onCreate: vi.fn(async () => null),
	onStart: vi.fn(async () => {}),
	onStop: vi.fn(async () => {}),
	onSwitchStop: vi.fn(async () => {}),
	onDone: vi.fn(async () => {}),
	descriptionForTask: vi.fn(async (_l: string, d: string | null) => d)
}));

import type * as TaskService from './taskService';
import type * as Db from './db';

let svc: typeof TaskService;
let dbmod: typeof Db;

const OWNER = 'u-integ-owner';
const OTHER = 'u-integ-other';
const DAY = '2026-09-21';

beforeAll(async () => {
	// Dynamic imports are load-bearing, not stylistic: `vi.mock('$env/dynamic/private')`
	// must be in place before `db/index.ts` reads DATABASE_PATH, and a static import
	// here would be evaluated during the module graph's hoist, before the mock
	// applies. Same reason the migrate call is here. See taskService.status.test.ts.
	const { execSync } = await import('node:child_process');
	execSync('pnpm db:migrate', { env: { ...process.env, DATABASE_PATH: dbPath }, stdio: 'ignore' });
	dbmod = await import('./db');
	svc = await import('./taskService');
	const { users } = await import('./db/schema');
	const now = new Date();
	dbmod.db.insert(users).values([
		{ id: OWNER, email: 'owner@example.com', passwordHash: 'x', createdAt: now },
		{ id: OTHER, email: 'other@example.com', passwordHash: 'x', createdAt: now }
	]).run();
});

afterAll(() => {
	rmSync(dir, { recursive: true, force: true });
});

async function makeTask(userId: string, label: string): Promise<number> {
	const task = await svc.createTask(userId, { label, description: null, workDate: DAY });
	return task!.id;
}

describe('task_integrations contract', () => {
	it('upserts one row per (task, group, field) instead of appending', async () => {
		const id = await makeTask(OWNER, 'INT-1');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'running');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'idle');

		const rows = svc.listIntegrations(OWNER, id)!.filter((r) => r.group === 'agent');
		expect(rows).toHaveLength(1);
		expect(rows[0].value).toBe('idle');
	});

	it('clears a value with null while keeping the row addressable', async () => {
		const id = await makeTask(OWNER, 'INT-2');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'running');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', null);

		const rows = svc.listIntegrations(OWNER, id)!.filter((r) => r.group === 'agent');
		expect(rows).toHaveLength(1);
		expect(rows[0].value).toBeNull();

		// Still addressable: the next write updates it rather than adding a second.
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'closed');
		const after = svc.listIntegrations(OWNER, id)!.filter((r) => r.group === 'agent');
		expect(after).toHaveLength(1);
		expect(after[0].value).toBe('closed');
	});

	it('keeps groups and fields independent on the same task', async () => {
		const id = await makeTask(OWNER, 'INT-3');
		svc.upsertIntegration(OWNER, id, 'jira', 'issue_key', 'US-1');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'running');
		svc.upsertIntegration(OWNER, id, 'agent', 'session', 'abc-123');

		const rows = svc.listIntegrations(OWNER, id)!;
		expect(rows).toHaveLength(3);
		expect(rows.map((r) => `${r.group}/${r.field}`).sort()).toEqual([
			'agent/session',
			'agent/status',
			'jira/issue_key'
		]);
	});

	it('refuses another user read and write on a task it does not own', async () => {
		const id = await makeTask(OWNER, 'INT-4');
		svc.upsertIntegration(OWNER, id, 'agent', 'status', 'running');

		// The service returns null for a task the user does not own; the route turns
		// that into a 404. Either way nothing leaks.
		expect(svc.listIntegrations(OTHER, id)).toBeNull();
		expect(svc.upsertIntegration(OTHER, id, 'agent', 'status', 'pwned')).toBeNull();

		const rows = svc.listIntegrations(OWNER, id)!;
		expect(rows).toHaveLength(1);
		expect(rows[0].value).toBe('running');
	});
});
