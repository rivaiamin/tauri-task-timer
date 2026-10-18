import { and, eq } from 'drizzle-orm';
import { spawnSync } from 'node:child_process';
import { db, schema, sqlite } from './db';
import * as jira from './jira';
import { upsertIntegration } from './taskService';
import { ingestJiraIssues } from './jiraIngest';
import { isDoneJiraStatus } from '$lib/kanbanColumns';
import type { KanbanCard, KanbanPullResponse, KanbanPipelineName } from './kanbanContracts';

export type { KanbanCard, KanbanPullResponse, KanbanPipelineName } from './kanbanContracts';

const { tasks, taskIntegrations } = schema;

export function monitorProjects(): string[] {
	const raw = process.env.JIRA_MONITOR_PROJECTS ?? 'US,AIMSIS';
	const parts = raw.split(',').map((s) => s.trim().toUpperCase()).filter(Boolean);
	return parts.length ? parts : ['US', 'AIMSIS'];
}

export function listKanbanCards(userId: string, workDate: string): KanbanCard[] {
	const stmt = sqlite.prepare(`
		SELECT t.id, t.label, k.value AS issue_key, COALESCE(s.value, '') AS jira_status,
		       COALESCE(td.is_running, 0) AS running
		FROM tasks t
		INNER JOIN task_integrations k ON k.task_id = t.id AND k."group" = 'jira' AND k.field = 'issue_key'
		LEFT JOIN task_integrations s ON s.task_id = t.id AND s."group" = 'jira' AND s.field = 'status'
		LEFT JOIN task_days td ON td.task_id = t.id AND td.work_date = ?
		WHERE t.user_id = ? AND k.value IS NOT NULL AND trim(k.value) != ''
	`);
	const rows = stmt.all(workDate, userId) as {
		id: number;
		label: string;
		issue_key: string;
		jira_status: string;
		running: number;
	}[];
	const out: KanbanCard[] = [];
	for (const row of rows) {
		const key = row.issue_key?.trim();
		if (!key) continue;
		const status = row.jira_status ?? '';
		if (isDoneJiraStatus(status)) continue;
		out.push({
			taskId: row.id,
			label: row.label,
			issueKey: key,
			jiraStatus: status,
			runningToday: row.running !== 0
		});
	}
	out.sort((a, b) => a.issueKey.localeCompare(b.issueKey));
	return out;
}

function listIssueKeys(userId: string): { taskId: number; key: string }[] {
	return db
		.select({ taskId: tasks.id, key: taskIntegrations.value })
		.from(tasks)
		.innerJoin(
			taskIntegrations,
			and(
				eq(taskIntegrations.taskId, tasks.id),
				eq(taskIntegrations.group, 'jira'),
				eq(taskIntegrations.field, 'issue_key')
			)
		)
		.where(eq(tasks.userId, userId))
		.all()
		.filter((r) => r.key && r.key.trim())
		.map((r) => ({ taskId: r.taskId, key: r.key!.trim() }));
}

export async function refreshKanbanStatuses(
	userId: string
): Promise<{ updated: { key: string; status: string }[]; warnings: string[] }> {
	const updated: { key: string; status: string }[] = [];
	const warnings: string[] = [];
	for (const { taskId, key } of listIssueKeys(userId)) {
		const issue = await jira.fetchIssueFields(key);
		if (!issue) {
			warnings.push(`${key}: fetch failed or jira unconfigured`);
			continue;
		}
		if (issue.status) {
			upsertIntegration(userId, taskId, 'jira', 'status', issue.status);
			updated.push({ key: issue.key, status: issue.status });
		}
	}
	return { updated, warnings };
}

export async function pullKanbanSprint(
	userId: string,
	workDate: string
): Promise<KanbanPullResponse> {
	const projects = monitorProjects();
	const jql = jira.jqlOpenSprintAssigneeUndone(projects);
	const issues = await jira.searchIssues(jql);
	ingestJiraIssues(
		userId,
		workDate,
		issues.map((issue) => ({
			key: issue.key,
			summary: issue.summary,
			status: issue.status
		}))
	);
	const refresh = await refreshKanbanStatuses(userId);
	const issuesProcessed = issues.length;
	return {
		ingested: issuesProcessed,
		issuesProcessed,
		warnings: refresh.warnings
	};
}

function resolveBin(envKey: string, fallback: string): string {
	return process.env[envKey]?.trim() || fallback;
}

export function runKanbanPipeline(
	name: KanbanPipelineName,
	opts: { dryRun?: boolean; days?: number; apply?: boolean; project?: string } = {}
): { exitCode: number; stdout: string; stderr: string } {
	const projects = monitorProjects();
	let bin: string;
	const args: string[] = [];
	if (name === 'local-ok') {
		bin = resolveBin('JIRA_PIPELINE_LOCAL_OK', 'jira-local-ok-pr-sync');
		if (projects.length === 1) args.push(projects[0]);
		else args.push('--all');
		if (opts.dryRun) args.push('--dry-run');
		args.push('--json');
	} else if (name === 'prod-done') {
		bin = resolveBin('JIRA_PIPELINE_PROD_DONE', 'jira-cek-di-prod-done');
		args.push(`--days=${opts.days ?? 3}`);
		if (opts.dryRun) args.push('--dry-run');
		if (opts.apply) args.push('--apply');
		args.push('--json');
		if (projects.length === 1) args.push(`--project=${projects[0]}`);
	} else {
		bin = resolveBin('JIRA_PIPELINE_UNASSIGNED', 'jira-unassigned-todo');
		if (opts.project) args.push(opts.project);
		args.push('--json');
	}
	const proc = spawnSync(bin, args, { encoding: 'utf8' });
	return {
		exitCode: proc.status ?? -1,
		stdout: proc.stdout ?? '',
		stderr: proc.stderr ?? ''
	};
}
