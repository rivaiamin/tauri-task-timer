import { json, error, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { pullKanbanSprint } from '$lib/server/kanban';
import { kanbanPullBodySchema } from '$lib/server/kanbanContracts';
import { todayISO } from '$lib/dates';

export const POST: RequestHandler = async (event) => {
	const actor = await resolveActor(event);
	requireScope(actor, 'tasks:write');
	const raw = await event.request.json().catch(() => ({}));
	const parsed = kanbanPullBodySchema.safeParse(raw);
	if (!parsed.success) {
		throw error(400, parsed.error.issues.map((i) => i.message).join('; '));
	}
	const workDate = parsed.data?.workDate ?? todayISO();
	const result = await pullKanbanSprint(actor.userId, workDate);
	return json(result);
};
