import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import { and, eq } from 'drizzle-orm';
import {
	AmbiguousJiraIdentityError,
	ingestJiraIssues
} from './jiraIngest';
import * as schema from './db/schema';

const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-jira-ingest-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));
vi.mock('./jira', () => ({
	onCreate: vi.fn(async () => null),
	onStart: vi.fn(),
	onStop: vi.fn(),
	onSwitchStop: vi.fn(),
	onDone: vi.fn(),
	descriptionForTask: vi.fn(async (_l: string, d: string | null) => d)
}));

let dbmod: typeof import('./db');

function seedUser(id: string) {
	dbmod.db
		.insert(schema.users)
		.values({ id, email: `${id}@test.com`, passwordHash: 'x', createdAt: new Date() })
		.run();
}

beforeAll(async () => {
	const { execSync } = await import('node:child_process');
	execSync('pnpm db:migrate', { env: { ...process.env, DATABASE_PATH: dbPath }, stdio: 'ignore' });
	dbmod = await import('./db');
});

afterAll(() => rmSync(dir, { recursive: true, force: true }));

function integrationRows(taskId: number) {
	return dbmod.db
		.select()
		.from(schema.taskIntegrations)
		.where(eq(schema.taskIntegrations.taskId, taskId))
		.all()
		.sort((a, b) => `${a.field}`.localeCompare(`${b.field}`));
}

describe('ingestJiraIssues', () => {
	it('reuses an existing bare-key task instead of creating a second', () => {
		const USER = 'u-reuse';
		const DAY = '2026-10-18';
		seedUser(USER);
		const { tasks, taskDays } = schema;
		dbmod.db.insert(tasks).values({ userId: USER, label: 'US-2449', createdAt: new Date(), updatedAt: new Date() }).run();
		const taskId = dbmod.db.select({ id: tasks.id }).from(tasks).where(eq(tasks.userId, USER)).get()!.id;
		dbmod.db
			.insert(taskDays)
			.values({ taskId, workDate: DAY, status: 'todo', position: 0, createdAt: new Date(), updatedAt: new Date() })
			.run();

		const rows = ingestJiraIssues(USER, DAY, [
			{ key: 'US-2449', summary: 'Upload survey files', status: 'In Progress' }
		]);
		expect(rows[0].taskId).toBe(taskId);
		expect(dbmod.db.select().from(tasks).where(eq(tasks.userId, USER)).all()).toHaveLength(1);
		const statusRow = integrationRows(taskId).find((r) => r.field === 'status');
		expect(statusRow?.value).toBe('In Progress');
	});

	it('refuses an ambiguous key match', () => {
		const USER = 'u-ambig';
		const DAY = '2026-10-19';
		seedUser(USER);
		const { tasks, taskDays } = schema;
		dbmod.db.insert(tasks).values({ userId: USER, label: 'US-7 alpha', createdAt: new Date(), updatedAt: new Date() }).run();
		dbmod.db.insert(tasks).values({ userId: USER, label: 'US-7 beta', createdAt: new Date(), updatedAt: new Date() }).run();
		for (const row of dbmod.db.select().from(tasks).where(eq(tasks.userId, USER)).all()) {
			dbmod.db
				.insert(taskDays)
				.values({ taskId: row.id, workDate: DAY, status: 'todo', position: 0, createdAt: new Date(), updatedAt: new Date() })
				.run();
		}
		expect(() =>
			ingestJiraIssues(USER, DAY, [{ key: 'US-7', summary: 'x', status: 'To Do' }])
		).toThrow(AmbiguousJiraIdentityError);
	});

	it('does not blank a stored status when fetch reports empty status', () => {
		const USER = 'u-status';
		const DAY = '2026-10-20';
		seedUser(USER);
		const { tasks, taskIntegrations } = schema;
		ingestJiraIssues(USER, DAY, [{ key: 'US-1459', summary: 'Fix login', status: 'In Progress' }]);
		const taskId = dbmod.db.select({ id: tasks.id }).from(tasks).where(eq(tasks.userId, USER)).get()!.id;
		ingestJiraIssues(USER, DAY, [{ key: 'US-1459', summary: 'Fix login', status: '' }]);
		const status = dbmod.db
			.select()
			.from(taskIntegrations)
			.where(and(eq(taskIntegrations.taskId, taskId), eq(taskIntegrations.field, 'status')))
			.get();
		expect(status?.value).toBe('In Progress');
	});

	it('kanban ingest does not add a day row for a new issue', () => {
		const USER = 'u-kanban-day';
		const DAY = '2026-10-21';
		seedUser(USER);
		const { taskDays } = schema;
		ingestJiraIssues(
			USER,
			DAY,
			[{ key: 'US-300', summary: 'Kanban only', status: 'To Do' }],
			'kanban'
		);
		const taskId = dbmod.db.select({ id: schema.tasks.id }).from(schema.tasks).where(eq(schema.tasks.userId, USER)).get()!
			.id;
		const days = dbmod.db.select().from(taskDays).where(eq(taskDays.taskId, taskId)).all();
		expect(days).toHaveLength(0);
	});

	it('kanban ingest reuses a task linked only by jira issue_key', () => {
		const USER = 'u-kanban-key';
		const DAY = '2026-10-22';
		seedUser(USER);
		const { tasks, taskIntegrations } = schema;
		dbmod.db
			.insert(tasks)
			.values({
				userId: USER,
				label: 'Fix login flow',
				createdAt: new Date(),
				updatedAt: new Date()
			})
			.run();
		const taskId = dbmod.db.select({ id: tasks.id }).from(tasks).where(eq(tasks.userId, USER)).get()!
			.id;
		dbmod.db
			.insert(taskIntegrations)
			.values({
				taskId,
				group: 'jira',
				field: 'issue_key',
				value: 'US-400',
				createdAt: new Date(),
				updatedAt: new Date()
			})
			.run();

		ingestJiraIssues(USER, DAY, [{ key: 'US-400', summary: 'From JIRA', status: 'In Progress' }], 'kanban');
		expect(dbmod.db.select().from(tasks).where(eq(tasks.userId, USER)).all()).toHaveLength(1);
		const status = integrationRows(taskId).find((r) => r.field === 'status');
		expect(status?.value).toBe('In Progress');
	});

	it('normalizes a legacy KEY summary label on reuse', () => {
		const USER = 'u-legacy';
		const DAY = '2026-10-12';
		seedUser(USER);
		const { tasks, taskDays } = schema;
		dbmod.db
			.insert(tasks)
			.values({
				userId: USER,
				label: 'US-2511 Old combined title',
				createdAt: new Date(),
				updatedAt: new Date()
			})
			.run();
		const taskId = dbmod.db
			.select({ id: tasks.id })
			.from(tasks)
			.where(and(eq(tasks.userId, USER), eq(tasks.label, 'US-2511 Old combined title')))
			.get()!.id;
		dbmod.db
			.insert(taskDays)
			.values({ taskId, workDate: DAY, status: 'todo', position: 0, createdAt: new Date(), updatedAt: new Date() })
			.run();

		ingestJiraIssues(USER, DAY, [
			{ key: 'US-2511', summary: 'Fresh summary from JIRA', status: 'To Do' }
		]);
		const row = dbmod.db.select().from(tasks).where(eq(tasks.id, taskId)).get();
		expect(row?.label).toBe('US-2511');
		expect(row?.description).toBe('Fresh summary from JIRA');
	});
});
