import type { PageServerLoad } from './$types';
import { listKanbanCards, monitorProjects } from '$lib/server/kanban';
import { todayISO } from '$lib/dates';
import { KANBAN_COLUMNS } from '$lib/kanbanColumns';

export const load: PageServerLoad = async ({ locals }) => {
	const user = locals.user!;
	const workDate = todayISO();
	return {
		workDate,
		cards: listKanbanCards(user.id, workDate),
		columns: KANBAN_COLUMNS,
		projects: monitorProjects()
	};
};
