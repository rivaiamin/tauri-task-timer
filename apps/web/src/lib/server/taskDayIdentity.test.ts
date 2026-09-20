// The day-identity contract, driven through the real service layer against a
// throwaway migrated DB.
//
// This is the oracle for "one task per label, one row per day worked": the same
// label created on two days must be ONE task with TWO day rows, each carrying
// its own recorded time, and the archive must return one entry per task with
// every day listed. It reads what the app returns, not what the SQL intended.
//
// Every case builds its own data and asserts only on it, so each one also holds
// when run alone (`vitest run --testNamePattern=...`).
//
// The env module is mocked because db/index.ts reads DATABASE_PATH from
// `$env/dynamic/private`, which Vite resolves from apps/web/.env and which would
// otherwise point this test at the real database.
import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import { eq } from 'drizzle-orm';
import * as schema from './db/schema';
import type * as TaskService from './taskService';
import type * as Db from './db';

const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-dayid-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));

// JIRA is stubbed so the test never reaches Atlassian; a label like "US-1"
// parses as an issue key and loadCreds() would otherwise read the developer's
// real ~/.aimsis/jira.env.
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

const USER = 'u-dayid';
const DAY1 = '2026-09-20';
const DAY2 = '2026-09-21';

beforeAll(async () => {
	svc = await import('./taskService');
	dbmod = await import('./db');
	dbmod.db
		.insert(schema.users)
		.values({ id: USER, email: 'dayid@test.com', passwordHash: 'x', createdAt: new Date() })
		.run();
});

afterAll(() => {
	rmSync(dir, { recursive: true, force: true });
});

/** Every day row of a task, oldest first. */
function dayRows(taskId: number) {
	return dbmod.db
		.select()
		.from(schema.taskDays)
		.where(eq(schema.taskDays.taskId, taskId))
		.all()
		.sort((a, b) => a.workDate.localeCompare(b.workDate));
}

/**
 * Work one label on the given days, giving each day its own recorded time.
 * Returns the task id every day resolved to.
 */
async function work(label: string, days: Array<[string, number]>): Promise<number> {
	let id = 0;
	for (const [workDate, seconds] of days) {
		const created = await svc.createTask(USER, { label, workDate, description: null });
		id = created.id;
		await svc.updateTask(USER, id, { elapsedSeconds: seconds }, workDate);
	}
	return id;
}

describe('one task per label, one row per day worked', () => {
	it('the same label on a new day is the same task with its own day time', async () => {
		const id = await work('SAME-1', [[DAY1, 3600], [DAY2, 1800]]);

		// One task, not two.
		const identities = dbmod.db
			.select()
			.from(schema.tasks)
			.where(eq(schema.tasks.label, 'SAME-1'))
			.all();
		expect(identities).toHaveLength(1);

		// Two days, each with its own time.
		const rows = dayRows(id);
		expect(rows.map((r) => r.workDate)).toEqual([DAY1, DAY2]);
		expect(rows.map((r) => r.elapsedTime)).toEqual([3600, 1800]);
	});

	it('the description lives on the task, so every day shares it', async () => {
		const first = await svc.createTask(USER, {
			label: 'DESC-1',
			description: 'shared description',
			workDate: DAY1
		});
		const second = await svc.createTask(USER, { label: 'DESC-1', workDate: DAY2, description: null });

		expect(second.id).toBe(first.id);
		expect(second.description).toBe('shared description');
		// And the stored identity holds it once, not once per day.
		expect(
			dbmod.db.select().from(schema.tasks).where(eq(schema.tasks.id, first.id)).get()!.description
		).toBe('shared description');
	});

	it('each day lists the task with only that day\u2019s time', async () => {
		const id = await work('LIST-1', [[DAY1, 3600], [DAY2, 1800]]);

		const onDay1 = svc.listTasks(USER, DAY1).tasks.filter((t) => t.label === 'LIST-1');
		expect(onDay1).toHaveLength(1);
		expect(onDay1[0].elapsedSeconds).toBe(3600);

		const onDay2 = svc.listTasks(USER, DAY2).tasks.filter((t) => t.label === 'LIST-1');
		expect(onDay2).toHaveLength(1);
		expect(onDay2[0].id).toBe(id);
		expect(onDay2[0].elapsedSeconds).toBe(1800);
	});

	it('a day the task was not worked does not list it', async () => {
		await work('ABSENT-1', [[DAY1, 60]]);
		const { tasks } = svc.listTasks(USER, '2026-09-19');
		expect(tasks.filter((t) => t.label === 'ABSENT-1')).toHaveLength(0);
	});

	it('starting on a new day adds that day without disturbing the old one', async () => {
		const id = await work('START-1', [[DAY1, 3600], [DAY2, 1800]]);
		const started = svc.startTimer(USER, id, '2026-09-22');
		expect(started).not.toBeNull();

		const rows = dayRows(id);
		expect(rows.map((r) => r.workDate)).toEqual([DAY1, DAY2, '2026-09-22']);
		// Only the new day is running; the earlier days keep their recorded time.
		expect(rows.map((r) => r.isRunning)).toEqual([false, false, true]);
		expect(rows.map((r) => r.elapsedTime)).toEqual([3600, 1800, 0]);
	});
});

describe('archive groups every day under one task', () => {
	it('returns one entry per task carrying each day worked', async () => {
		const id = await work('ARCH-1', [[DAY1, 3600], [DAY2, 1800], ['2026-09-22', 600]]);

		const us = svc.listArchive(USER, {}).tasks.filter((t) => t.label === 'ARCH-1');
		// One entry, not one per day.
		expect(us).toHaveLength(1);
		expect(us[0].id).toBe(id);

		const days = us[0].days ?? [];
		expect(days.map((d) => d.workDate)).toEqual([DAY1, DAY2, '2026-09-22']);
		// Each day carries its own time, not the task's total.
		const byDate = Object.fromEntries(days.map((d) => [d.workDate, d.elapsedSeconds]));
		expect(byDate[DAY1]).toBe(3600);
		expect(byDate[DAY2]).toBe(1800);
		expect(byDate['2026-09-22']).toBe(600);
	});

	it('a task appears exactly once even when it was worked many days', async () => {
		await work('ONCE-1', [[DAY1, 60], [DAY2, 60], ['2026-09-22', 60]]);
		const matches = svc.listArchive(USER, {}).tasks.filter((t) => t.label === 'ONCE-1');
		expect(matches).toHaveLength(1);
		expect(matches[0].days).toHaveLength(3);
	});
});

describe('task-level records are not duplicated per day', () => {
	it('a JIRA key written on one day stays one row for the task', async () => {
		const id = await work('KEY-1', [[DAY1, 60], [DAY2, 60]]);
		svc.upsertIntegration(USER, id, 'jira', 'issue_key', 'KEY-1');
		// Upserting again (as a second day's ingest would) must not add a row.
		svc.upsertIntegration(USER, id, 'jira', 'issue_key', 'KEY-1');

		const rows = dbmod.db
			.select()
			.from(schema.taskIntegrations)
			.where(eq(schema.taskIntegrations.taskId, id))
			.all();
		expect(rows).toHaveLength(1);
		expect(rows[0].value).toBe('KEY-1');
	});

	it('deleting a task takes its days with it', async () => {
		const id = await work('GONE-1', [[DAY1, 60], [DAY2, 60]]);
		expect(svc.deleteTask(USER, id)).toBe(true);
		expect(dayRows(id)).toHaveLength(0);
		expect(svc.listArchive(USER, {}).tasks.filter((t) => t.label === 'GONE-1')).toHaveLength(0);
	});
});

describe('renaming onto a label the user already has', () => {
	it('is refused with a nameable conflict rather than a driver error', async () => {
		await work('CLASH-A', [[DAY1, 60]]);
		const b = await work('CLASH-B', [[DAY1, 60]]);

		// The identity's unique index would reject this; the service must say so
		// itself, because the raw driver error would reach the client as a 500.
		expect(() => svc.updateTask(USER, b, { label: 'CLASH-A' }, DAY1)).toThrow(
			svc.LabelConflictError
		);

		// And the refused rename must not have written anything.
		expect(
			dbmod.db.select().from(schema.tasks).where(eq(schema.tasks.id, b)).get()!.label
		).toBe('CLASH-B');
	});

	it('still allows renaming to a label nobody has, and to its own label', async () => {
		const a = await work('RENAME-1', [[DAY1, 60]]);
		expect(svc.updateTask(USER, a, { label: 'RENAME-2' }, DAY1)!.label).toBe('RENAME-2');
		// Renaming to the same label it already holds is not a conflict with itself.
		expect(svc.updateTask(USER, a, { label: 'RENAME-2' }, DAY1)!.label).toBe('RENAME-2');
	});

	it('lets a different user hold the same label', async () => {
		dbmod.db
			.insert(schema.users)
			.values({ id: 'u-other', email: 'other@test.com', passwordHash: 'x', createdAt: new Date() })
			.run();
		await work('SHARED-1', [[DAY1, 60]]);
		const theirs = await svc.createTask('u-other', {
			label: 'SHARED-1',
			description: null,
			workDate: DAY1
		});
		// The unique index is per user, so the other user's task is its own.
		expect(svc.updateTask('u-other', theirs.id, { label: 'SHARED-1' }, DAY1)!.label).toBe(
			'SHARED-1'
		);
	});
});
