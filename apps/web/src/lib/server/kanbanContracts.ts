import { z } from 'zod';

/** ISO calendar date used for task-day rows and `runningToday` on the board. */
export const ISO_DATE_REGEX = /^\d{4}-\d{2}-\d{2}$/;

export const isoDateString = z.string().regex(ISO_DATE_REGEX);

export interface KanbanCard {
	taskId: number;
	label: string;
	issueKey: string;
	jiraStatus: string;
	runningToday: boolean;
}

export interface KanbanListResponse {
	cards: KanbanCard[];
	columns: string[];
	projects: string[];
	/** Same concept as `workDate` on pull; drives `runningToday` on cards. */
	workDate: string;
}

export interface KanbanRefreshResponse {
	updated: Array<{ key: string; status: string }>;
	warnings: string[];
}

/**
 * Sprint pull report. `ingested` and `issuesProcessed` are the same count: Jira search
 * hits passed through ingest (includes idempotent re-pulls), not net-new tasks.
 */
export interface KanbanPullResponse {
	/** @deprecated Prefer `issuesProcessed`; kept for CLI/MCP compatibility. */
	ingested: number;
	issuesProcessed: number;
	warnings: string[];
}

export type KanbanPipelineName = 'local-ok' | 'prod-done' | 'unassigned';

export interface KanbanPipelineResponse {
	skipped?: boolean;
	exitCode: number;
	stdout: string;
	stderr: string;
}

/** Jira project key (monitor list uses uppercase). */
export const JIRA_PROJECT_KEY_REGEX = /^[A-Z][A-Z0-9]*$/;

export const kanbanPullBodySchema = z
	.object({
		workDate: isoDateString.optional()
	})
	.optional();

const pipelineBaseSchema = z.object({
	pipeline: z.enum(['local-ok', 'prod-done', 'unassigned']),
	dryRun: z.boolean().optional(),
	days: z.number().int().min(1).max(30).optional(),
	apply: z.boolean().optional(),
	project: z.string().optional()
});

export type KanbanPipelineInput = z.infer<typeof pipelineBaseSchema>;

export function resolveKanbanWorkDate(
	dateParam: string | null,
	workDateParam: string | null,
	fallback: string
): string {
	const candidate = workDateParam ?? dateParam;
	if (candidate && ISO_DATE_REGEX.test(candidate)) return candidate;
	return fallback;
}

export function parseKanbanPipelineBody(
	raw: unknown,
	monitorProjects: string[]
): { success: true; data: KanbanPipelineInput } | { success: false; message: string } {
	const parsed = pipelineBaseSchema.safeParse(raw);
	if (!parsed.success) {
		return { success: false, message: parsed.error.issues.map((i) => i.message).join('; ') };
	}
	const { pipeline, project } = parsed.data;
	const allowed = new Set(monitorProjects.map((p) => p.toUpperCase()));

	if (pipeline === 'unassigned') {
		if (!project?.trim()) {
			return { success: false, message: 'project is required for unassigned pipeline' };
		}
	} else if (project !== undefined && project !== '') {
		return {
			success: false,
			message: 'project is only used with the unassigned pipeline'
		};
	}

	if (project !== undefined && project !== '') {
		const key = project.trim().toUpperCase();
		if (!JIRA_PROJECT_KEY_REGEX.test(key)) {
			return { success: false, message: 'project must be an uppercase Jira project key' };
		}
		if (!allowed.has(key)) {
			return {
				success: false,
				message: `project must be one of: ${[...allowed].sort().join(', ')}`
			};
		}
		parsed.data.project = key;
	}

	return { success: true, data: parsed.data };
}
