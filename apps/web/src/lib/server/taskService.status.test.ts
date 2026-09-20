// Integration test for the status wiring in taskService — the layer the pure
// taskStatus unit test cannot see. This is the web half of the "timer start
// moves the status" contract: it drives the real start/stop/reset/done paths
// against a throwaway migrated DB, so removing an autoStatus call fails here.
//
// The env module is mocked because db/index.ts reads DATABASE_PATH from
// `$env/dynamic/private`, which Vite resolves from apps/web/.env
// (`DATABASE_PATH=local.db`) and which would otherwise point the test at the
// real database. The mock is set up before the dynamic imports below.
import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import { and, eq } from 'drizzle-orm';
import * as schema from './db/schema';
import type * as TaskService from './taskService';
import type * as Db from './db';

// vi.hoisted runs before the hoisted vi.mock factory, so the temp path is
// available when the env module is stubbed.
const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-status-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));

// The JIRA hooks are stubbed so this test never reaches Atlassian: a label like
// "WIRE-1" parses as an issue key, and loadCreds() would otherwise pick up the
// developer's real ~/.aimsis/jira.env. The status wiring is what is under test.
vi.mock('./jira', () => ({
	onCreate: vi.fn(async () => null),
	onStart: vi.fn(async () => {}),
	onStop: vi.fn(async () => {}),
	onSwitchStop: vi.fn(async () => {}),
	onDone: vi.fn(async () => {}),
	descriptionForTask: vi.fn(async (_l: string, d: string | null) => d)
}));

let svc: typeof TaskService;
let dbmod: typeof Db;

const USER = 'u-status';
const DAY = '2026-09-21';

beforeAll(async () => {
	svc = await import('./taskService');
	dbmod = await import('./db');
	// A user row is required by the FK on tasks.
	dbmod.db
		.insert(schema.users)
		.values({ id: USER, email: 'status@test.com', passwordHash: 'x', createdAt: new Date() })
		.run();
});

afterAll(() => {
	rmSync(dir, { recursive: true, force: true });
});

/** The status of the task's day row on the viewed day. */
function statusOf(taskId: number): string {
	return dbmod.db
		.select({ status: schema.taskDays.status })
		.from(schema.taskDays)
		.where(and(eq(schema.taskDays.taskId, taskId), eq(schema.taskDays.workDate, DAY)))
		.get()!.status;
}

async function newTask(label: string): Promise<number> {
	const t = await svc.createTask(USER, { label, workDate: DAY, description: null });
	return t.id;
}

describe('taskService status wiring', () => {
	it('moves the status through start → stop → done → un-done', async () => {
		const id = await newTask('WIRE-1');
		expect(statusOf(id)).toBe('todo'); // schema default

		svc.startTimer(USER, id, DAY);
		expect(statusOf(id)).toBe('21');

		svc.stopTimer(USER, id, DAY);
		expect(statusOf(id)).toBe('11');

		svc.updateTask(USER, id, { done: true }, DAY);
		expect(statusOf(id)).toBe('31');

		svc.updateTask(USER, id, { done: false }, DAY);
		expect(statusOf(id)).toBe('11');
	});

	it('returns the status to To Do on reset and reset-all', async () => {
		const id = await newTask('WIRE-2');
		svc.startTimer(USER, id, DAY);
		expect(statusOf(id)).toBe('21');
		svc.resetTask(USER, id, DAY);
		expect(statusOf(id)).toBe('11');

		svc.startTimer(USER, id, DAY);
		expect(statusOf(id)).toBe('21');
		svc.resetAll(USER, DAY);
		expect(statusOf(id)).toBe('11');
	});

	it('keeps Done through a stop, and demotes a focus-switch victim', async () => {
		// A done task that runs and stops is still done.
		const done = await newTask('WIRE-3');
		svc.updateTask(USER, done, { done: true }, DAY);
		svc.startTimer(USER, done, DAY);
		expect(statusOf(done)).toBe('21');
		svc.stopTimer(USER, done, DAY);
		expect(statusOf(done)).toBe('31');

		// Focus mode: starting B stops A, and A must not stay In Progress.
		svc.setTimerMode(USER, 'focus');
		const a = await newTask('WIRE-4A');
		const b = await newTask('WIRE-4B');
		svc.startTimer(USER, a, DAY);
		expect(statusOf(a)).toBe('21');
		svc.startTimer(USER, b, DAY);
		expect(statusOf(a)).toBe('11');
		expect(statusOf(b)).toBe('21');
		svc.setTimerMode(USER, 'parallel');
	});

	it('lets an explicit status in the same PATCH win over the automatic write', async () => {
		const id = await newTask('WIRE-5');
		svc.updateTask(USER, id, { done: true, status: '81' }, DAY);
		expect(statusOf(id)).toBe('81');
	});
});
