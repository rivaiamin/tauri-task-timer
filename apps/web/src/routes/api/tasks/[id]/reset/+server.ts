import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { resetTask } from '$lib/server/taskService';

const bodySchema = z.object({
  workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
});

// POST /api/tasks/:id/reset — reset one day's elapsed time to 0.
// Body: { workDate? } — omit to reset whatever is running, or today's row.
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const parsed = bodySchema.safeParse(await event.request.json().catch(() => ({})));
  if (!parsed.success) throw error(400, 'Invalid body: workDate must be a YYYY-MM-DD date');

  const task = resetTask(actor.userId, id, parsed.data.workDate);
  if (!task) throw error(404, 'Task not found');
  return json(task);
};
