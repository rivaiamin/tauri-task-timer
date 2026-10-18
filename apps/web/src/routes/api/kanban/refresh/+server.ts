import { json, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { refreshKanbanStatuses } from '$lib/server/kanban';

export const POST: RequestHandler = async (event) => {
	const actor = await resolveActor(event);
	requireScope(actor, 'tasks:write');
	const result = await refreshKanbanStatuses(actor.userId);
	return json(result);
};
