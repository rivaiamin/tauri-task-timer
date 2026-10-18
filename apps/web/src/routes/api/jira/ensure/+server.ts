import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { ensureJiraIssue, JiraEnsureError } from '$lib/server/jiraEnsure';
import { AmbiguousJiraIdentityError } from '$lib/server/jiraIngest';
import { todayISO } from '$lib/dates';

const bodySchema = z.object({
	issueKey: z.string().trim().min(1).max(50),
	workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
});

export const POST: RequestHandler = async (event) => {
	const actor = await resolveActor(event);
	requireScope(actor, 'tasks:write');
	const parsed = bodySchema.safeParse(await event.request.json().catch(() => null));
	if (!parsed.success) {
		throw error(400, parsed.error.issues.map((i) => i.message).join('; '));
	}
	const workDate = parsed.data.workDate ?? todayISO();
	try {
		const task = await ensureJiraIssue(actor.userId, workDate, parsed.data.issueKey);
		return json({ task });
	} catch (err) {
		if (err instanceof JiraEnsureError) {
			if (err.code === 'invalid_key') throw error(400, err.message);
			throw error(502, err.message);
		}
		if (err instanceof AmbiguousJiraIdentityError) throw error(409, err.message);
		throw err;
	}
};
