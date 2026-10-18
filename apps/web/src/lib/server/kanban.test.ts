import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { rmSync } from 'node:fs';
import {
	parseKanbanPipelineBody,
	resolveKanbanWorkDate,
	type KanbanPullResponse
} from './kanbanContracts';

const { dir, dbPath } = vi.hoisted(() => {
  const fs = require('node:fs') as typeof import('node:fs');
  const os = require('node:os') as typeof import('node:os');
  const path = require('node:path') as typeof import('node:path');
  const d = fs.mkdtempSync(path.join(os.tmpdir(), 'tt-kanban-'));
  return { dir: d, dbPath: path.join(d, 'test.db') };
});

vi.mock('$env/dynamic/private', () => ({ env: { DATABASE_PATH: dbPath } }));
vi.mock('./jira', () => ({
  fetchIssueFields: vi.fn(),
  searchIssues: vi.fn(async () => [
    { key: 'US-1', summary: 'One', status: 'To Do' },
    { key: 'US-2', summary: 'Two', status: 'In Progress' }
  ]),
  jqlOpenSprintAssigneeUndone: vi.fn(() => 'jql')
}));

let svc: typeof import('./taskService');
let kanban: typeof import('./kanban');
const USER = 'u-kanban';
const DAY = '2026-10-18';

beforeAll(async () => {
  const { execSync } = await import('node:child_process');
  execSync('pnpm db:migrate', { env: { ...process.env, DATABASE_PATH: dbPath }, stdio: 'ignore' });
  const dbmod = await import('./db');
  svc = await import('./taskService');
  kanban = await import('./kanban');
  const { users } = await import('./db/schema');
  const now = new Date();
  dbmod.db.insert(users).values({ id: USER, email: 'k@example.com', passwordHash: 'x', createdAt: now }).run();
});

afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('kanban contracts', () => {
  it('resolves workDate query alias over date', () => {
    expect(resolveKanbanWorkDate('2026-01-01', '2026-02-02', DAY)).toBe('2026-02-02');
    expect(resolveKanbanWorkDate('2026-01-01', null, DAY)).toBe('2026-01-01');
    expect(resolveKanbanWorkDate('bad', null, DAY)).toBe(DAY);
  });

  it('requires project for unassigned pipeline', () => {
    const r = parseKanbanPipelineBody({ pipeline: 'unassigned' }, ['US']);
    expect(r.success).toBe(false);
    if (!r.success) expect(r.message).toMatch(/required/);
  });

  it('rejects project on local-ok', () => {
    const r = parseKanbanPipelineBody({ pipeline: 'local-ok', project: 'US' }, ['US']);
    expect(r.success).toBe(false);
  });

  it('accepts monitor project for unassigned', () => {
    const r = parseKanbanPipelineBody({ pipeline: 'unassigned', project: 'us' }, ['US', 'AIMSIS']);
    expect(r.success).toBe(true);
    if (r.success) expect(r.data.project).toBe('US');
  });
});

describe('kanban cards', () => {
  it('lists non-done jira-linked tasks', async () => {
    const task = await svc.createTask(USER, { label: 'US-99 fix', description: null, workDate: DAY });
    svc.upsertIntegration(USER, task.id, 'jira', 'issue_key', 'US-99');
    svc.upsertIntegration(USER, task.id, 'jira', 'status', 'To Do');
    const cards = kanban.listKanbanCards(USER, DAY);
    expect(cards).toHaveLength(1);
    expect(cards[0].issueKey).toBe('US-99');
  });
});

describe('kanban pull', () => {
  it('sets ingested equal to issuesProcessed', async () => {
    const result: KanbanPullResponse = await kanban.pullKanbanSprint(USER, DAY);
    expect(result.issuesProcessed).toBe(2);
    expect(result.ingested).toBe(result.issuesProcessed);
  });
});

describe('kanban refresh', () => {
  it('writes jira status from fetchIssueFields', async () => {
    const jiraMod = await import('./jira');
    const fetch = vi.mocked(jiraMod.fetchIssueFields);
    fetch.mockReset();
    fetch.mockImplementation(async (key: string) => {
      if (key === 'US-88') return { key: 'US-88', summary: 'Refresh me', status: 'Local OK' };
      return null;
    });
    const task = await svc.createTask(USER, { label: 'US-88', description: null, workDate: DAY });
    svc.upsertIntegration(USER, task.id, 'jira', 'issue_key', 'US-88');
    svc.upsertIntegration(USER, task.id, 'jira', 'status', 'To Do');
    const result = await kanban.refreshKanbanStatuses(USER);
    expect(result.updated).toContainEqual({ key: 'US-88', status: 'Local OK' });
    expect(result.warnings.some((w) => w.startsWith('US-88:'))).toBe(false);
  });
});
