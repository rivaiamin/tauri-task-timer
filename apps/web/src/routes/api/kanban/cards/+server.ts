import { json, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { listKanbanCards, monitorProjects } from '$lib/server/kanban';
import { resolveKanbanWorkDate } from '$lib/server/kanbanContracts';
import { todayISO } from '$lib/dates';
import { KANBAN_COLUMNS } from '$lib/kanbanColumns';

export const GET: RequestHandler = async (event) => {
	const actor = await resolveActor(event);
	requireScope(actor, 'tasks:read');
	const workDate = resolveKanbanWorkDate(
		event.url.searchParams.get('date'),
		event.url.searchParams.get('workDate'),
		todayISO()
	);
	const cards = listKanbanCards(actor.userId, workDate);
	return json({ cards, columns: KANBAN_COLUMNS, projects: monitorProjects(), workDate });
};
