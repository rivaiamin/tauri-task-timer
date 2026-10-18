// Port of `apps/tui/src/db/tasks.rs` `ingest_jira_issues` and identity lookup.
// Single writer for kanban pull and future web `jira ensure`.
import { and, eq } from 'drizzle-orm';
import { db, schema, sqlite } from './db';
import { ensureDayRow, insertIdentity } from './taskCreate';
import { upsertIntegration } from './taskService';
import type { Task } from './db/schema';

const { tasks } = schema;

export interface JiraIssueInput {
	key: string;
	summary: string;
	status: string;
}

export class AmbiguousJiraIdentityError extends Error {
	constructor(
		readonly selector: string,
		readonly taskIds: number[]
	) {
		super(`ambiguous label ${selector}: ids ${taskIds.join(', ')} (use numeric id)`);
		this.name = 'AmbiguousJiraIdentityError';
	}
}

type IdentityRow = { id: number; label: string };

const IDENTITY_DAY_JOIN = `INNER JOIN task_days td ON td.id = (
  SELECT MAX(d2.id) FROM task_days d2 WHERE d2.task_id = t.id
)`;

function findIdentitiesByExactLabel(userId: string, label: string): IdentityRow[] {
	return sqlite
		.prepare(
			`SELECT t.id, t.label FROM tasks t ${IDENTITY_DAY_JOIN}
       WHERE t.user_id = ? AND t.label = ? COLLATE NOCASE
       ORDER BY t.id ASC`
		)
		.all(userId, label) as IdentityRow[];
}

function escapeLikePrefix(prefix: string): string {
	return `${prefix.replace(/\\/g, '\\\\').replace(/%/g, '\\%').replace(/_/g, '\\_')} %`;
}

function findIdentitiesByLabelPrefix(userId: string, prefix: string): IdentityRow[] {
	const pattern = escapeLikePrefix(prefix);
	return sqlite
		.prepare(
			`SELECT t.id, t.label FROM tasks t ${IDENTITY_DAY_JOIN}
       WHERE t.user_id = ? AND t.label LIKE ? ESCAPE '\\' COLLATE NOCASE
       ORDER BY t.id ASC`
		)
		.all(userId, pattern) as IdentityRow[];
}

/** Same resolution order as TUI `find_task_identity` for a Jira issue key. */
export function findTaskIdentityByIssueKey(userId: string, key: string): Task | undefined {
	const selector = key.trim();
	if (!selector) return undefined;

	const exact = findIdentitiesByExactLabel(userId, selector);
	if (exact.length === 1) {
		return db.select().from(tasks).where(and(eq(tasks.id, exact[0].id), eq(tasks.userId, userId))).get();
	}
	if (exact.length > 1) {
		throw new AmbiguousJiraIdentityError(selector, exact.map((r) => r.id));
	}

	const prefixed = findIdentitiesByLabelPrefix(userId, selector);
	if (prefixed.length === 1) {
		return db.select().from(tasks).where(and(eq(tasks.id, prefixed[0].id), eq(tasks.userId, userId))).get();
	}
	if (prefixed.length > 1) {
		throw new AmbiguousJiraIdentityError(selector, prefixed.map((r) => r.id));
	}
	return undefined;
}

/** Rename `KEY summary` labels to bare `KEY` with summary in description (TUI parity). */
export function normalizeLegacyJiraLabel(
	userId: string,
	taskId: number,
	key: string,
	summary: string
): void {
	const row = db
		.select({ label: tasks.label, description: tasks.description })
		.from(tasks)
		.where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
		.get();
	if (!row) return;
	if (row.label.toLowerCase() === key.toLowerCase()) return;

	const prefix = `${key} `;
	if (!row.label.toLowerCase().startsWith(prefix.toLowerCase())) return;

	const desc = summary.trim();
	let nextDescription = row.description;
	if (desc) nextDescription = desc;
	else if (!row.description?.trim()) {
		const tail = row.label.slice(prefix.length).trim();
		nextDescription = tail || null;
	}
	db.update(tasks)
		.set({ label: key, description: nextDescription, updatedAt: new Date() })
		.where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
		.run();
}

/**
 * Store Jira issues as timer tasks with `jira/issue_key` + `jira/status` rows.
 * Reuses existing identities; refuses ambiguous key matches.
 */
export function ingestJiraIssues(
	userId: string,
	workDate: string,
	issues: JiraIssueInput[]
): { taskId: number }[] {
	const out: { taskId: number }[] = [];
	for (const { key, summary, status } of issues) {
		const existing = findTaskIdentityByIssueKey(userId, key);
		let taskId: number;
		if (existing) {
			normalizeLegacyJiraLabel(userId, existing.id, key, summary);
			ensureDayRow(db, userId, existing.id, workDate);
			taskId = existing.id;
		} else {
			const created = insertIdentity(db, userId, {
				label: key,
				workDate,
				description: summary.trim() || null
			});
			ensureDayRow(db, userId, created.id, workDate);
			taskId = created.id;
		}
		upsertIntegration(userId, taskId, 'jira', 'issue_key', key);
		if (status.trim()) {
			upsertIntegration(userId, taskId, 'jira', 'status', status.trim());
		}
		out.push({ taskId });
	}
	return out;
}
