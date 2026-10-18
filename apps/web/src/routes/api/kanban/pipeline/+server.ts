import { json, error, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { runKanbanPipeline, monitorProjects } from '$lib/server/kanban';
import { parseKanbanPipelineBody } from '$lib/server/kanbanContracts';

export const POST: RequestHandler = async (event) => {
	const actor = await resolveActor(event);
	requireScope(actor, 'tasks:write');
	const parsed = parseKanbanPipelineBody(await event.request.json().catch(() => null), monitorProjects());
	if (!parsed.success) throw error(400, parsed.message);
	if (process.env.SKIP_PIPELINE === '1') {
		return json({ skipped: true, exitCode: 0, stdout: '', stderr: '' });
	}
	const { pipeline, dryRun, days, apply, project } = parsed.data;
	const out = runKanbanPipeline(pipeline, { dryRun, days, apply, project });
	if (out.exitCode !== 0) throw error(502, out.stderr || `pipeline ${pipeline} failed`);
	return json(out);
};
