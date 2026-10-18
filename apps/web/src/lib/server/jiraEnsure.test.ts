import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import { eq } from 'drizzle-orm';
import { parseJiraIssueKey } from './jiraIngest';
import { ensureJiraIssue, JiraEnsureError } from './jiraEnsure';
import * as schema from './db/schema';

const { dir, dbPath } = vi.hoisted(() => {
	const fs = require('node:fs') as typeof import('node:fs');
	const os = require('node:os') as typeof import('node:os');
	const path = require('node:path') as typeof import('node:path');
	const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-jira-ensure-'));
	return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));
vi.mock('./jira', () => ({
	fetchIssueFields: vi.fn(async (key: string) => ({
		key,
		summary: 'Summary from Jira',
		status: 'To Do'
	})),
	onCreate: vi.fn(async () => null),
	onStart: vi.fn(),
	onStop: vi.fn(),
	onSwitchStop: vi.fn(),
	onDone: vi.fn(),
	descriptionForTask: vi.fn(async (_l: string, d: string | null) => d)
}));

const USER = 'u-ensure';
const DAY = '2026-10-18';

beforeAll(async () => {
	const { execSync } = await import('node:child_process');
	execSync('pnpm db:migrate', { env: { ...process.env, DATABASE_PATH: dbPath }, stdio: 'ignore' });
	const dbmod = await import('./db');
	dbmod.db
		.insert(schema.users)
		.values({ id: USER, email: 'ensure@test.com', passwordHash: 'x', createdAt: new Date() })
		.run();
});

afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('parseJiraIssueKey', () => {
	it('accepts a bare project key', () => {
		expect(parseJiraIssueKey('US-1459')).toBe('US-1459');
	});
	it('rejects non-keys without network', () => {
		expect(parseJiraIssueKey('not-a-key')).toBeNull();
	});
});

describe('ensureJiraIssue', () => {
	it('creates task identity and jira integration rows', async () => {
		const task = await ensureJiraIssue(USER, DAY, 'US-200');
		expect(task.label).toBe('US-200');
		expect(task.workDate).toBe(DAY);
		const dbmod = await import('./db');
		const rows = dbmod.db
			.select()
			.from(schema.taskIntegrations)
			.where(eq(schema.taskIntegrations.taskId, task.id))
			.all();
		expect(rows.some((r) => r.field === 'issue_key' && r.value === 'US-200')).toBe(true);
	});

	it('throws invalid_key for bad selector', async () => {
		await expect(ensureJiraIssue(USER, DAY, 'nope')).rejects.toMatchObject({
			code: 'invalid_key'
		} satisfies Partial<JiraEnsureError>);
	});
});
