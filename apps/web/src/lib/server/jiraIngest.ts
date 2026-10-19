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

/** Same key detection as TUI `jira::issue_key` (selector may be a bare key or label text). */
export function parseJiraIssueKey(selector: string): string | null {
	const words: string[] = [];
	for (const part of selector.trim().split(/\s+/)) {
		if (part) words.push(part);
	}
	for (const word of words) {
		const key = word.replace(/^[^\w-]+|[^\w-]+$/g, '');
		const dash = key.indexOf('-');
		if (dash <= 0) continue;
		const project = key.slice(0, dash);
		const number = key.slice(dash + 1);
		if (
			project.length > 0 &&
			[...project].every((c) => (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')) &&
			number.length > 0 &&
			[...number].every((c) => c >= '0' && c <= '9')
		) {
			return key;
		}
	}
	return null;
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

function findIdentitiesByExactLabel(userId: string, label: string): IdentityRow[] {
	return sqlite
		.prepare(
			`SELECT t.id, t.label FROM tasks t
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
			`SELECT t.id, t.label FROM tasks t
       WHERE t.user_id = ? AND t.label LIKE ? ESCAPE '\\' COLLATE NOCASE
       ORDER BY t.id ASC`
		)
		.all(userId, pattern) as IdentityRow[];
}

function findTaskIdsByIssueKey(userId: string, key: string): number[] {
	const rows = sqlite
		.prepare(
			`SELECT t.id FROM tasks t
       INNER JOIN task_integrations k ON k.task_id = t.id
         AND k."group" = 'jira' AND k.field = 'issue_key' AND k.value = ? COLLATE NOCASE
       WHERE t.user_id = ?
       ORDER BY t.id ASC`
		)
		.all(key, userId) as { id: number }[];
	return rows.map((r) => r.id);
}

/** Same resolution order as TUI `resolve_jira_issue_identity` for a Jira issue key. */
export function findTaskIdentityByIssueKey(userId: string, key: string): Task | undefined {
	const selector = key.trim();
	if (!selector) return undefined;

	const byIntegration = findTaskIdsByIssueKey(userId, selector);
	if (byIntegration.length === 1) {
		return db
			.select()
			.from(tasks)
			.where(and(eq(tasks.id, byIntegration[0]), eq(tasks.userId, userId)))
			.get();
	}
	if (byIntegration.length > 1) {
		throw new AmbiguousJiraIdentityError(selector, byIntegration);
	}

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

export type JiraIngestMode = 'timer' | 'kanban';

/**
 * Store Jira issues as timer tasks with `jira/issue_key` + `jira/status` rows.
 * Reuses existing identities; refuses ambiguous key matches.
 *
 * `timer` — also ensures a `task_days` row (sprint picker, `jira ensure`).
 * `kanban` — identity + integrations only; does not add to the daily list.
 */
export function ingestJiraIssues(
	userId: string,
	workDate: string,
	issues: JiraIssueInput[],
	mode: JiraIngestMode = 'timer'
): { taskId: number }[] {
	const ensureDay = mode === 'timer';
	const out: { taskId: number }[] = [];
	for (const { key, summary, status } of issues) {
		const existing = findTaskIdentityByIssueKey(userId, key);
		let taskId: number;
		if (existing) {
			normalizeLegacyJiraLabel(userId, existing.id, key, summary);
			if (ensureDay) {
				ensureDayRow(db, userId, existing.id, workDate);
			}
			taskId = existing.id;
		} else {
			const created = insertIdentity(db, userId, {
				label: key,
				workDate,
				description: summary.trim() || null
			});
			if (ensureDay) {
				ensureDayRow(db, userId, created.id, workDate);
			}
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
