import * as jira from './jira';
import { AmbiguousJiraIdentityError, ingestJiraIssues, parseJiraIssueKey } from './jiraIngest';
import { getTaskDtoForWorkDate } from './taskService';

export class JiraEnsureError extends Error {
	constructor(
		readonly code: 'invalid_key' | 'fetch_failed',
		message: string
	) {
		super(message);
		this.name = 'JiraEnsureError';
	}
}

/** Fetch one Jira issue and materialize it as a timer task (TUI `jira ensure`). */
export async function ensureJiraIssue(userId: string, workDate: string, selector: string) {
	const key = parseJiraIssueKey(selector);
	if (!key) {
		throw new JiraEnsureError('invalid_key', 'not a JIRA key');
	}
	const issue = await jira.fetchIssueFields(key);
	if (!issue) {
		throw new JiraEnsureError('fetch_failed', `fetch JIRA issue ${key} failed or jira unconfigured`);
	}
	let ingested: { taskId: number }[];
	try {
		ingested = ingestJiraIssues(userId, workDate, [
			{ key: issue.key, summary: issue.summary, status: issue.status }
		]);
	} catch (err) {
		if (err instanceof AmbiguousJiraIdentityError) throw err;
		throw err;
	}
	const taskId = ingested[0]?.taskId;
	const task = taskId ? getTaskDtoForWorkDate(userId, taskId, workDate) : undefined;
	if (!task) {
		throw new JiraEnsureError('fetch_failed', `task not found after ingest for ${key}`);
	}
	return task;
}
