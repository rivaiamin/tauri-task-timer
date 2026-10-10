import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { startTimer } from '$lib/server/taskService';

const bodySchema = z.object({
  exclusive: z.boolean().optional(),
  workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
});

// POST /api/tasks/:id/start — start a timer on a day (default today), adding that
// day to the task if it does not have one yet.
// Body: { exclusive?, workDate? } — omit `exclusive` to use the user's timer mode.
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const parsed = bodySchema.safeParse(await event.request.json().catch(() => ({})));
  if (!parsed.success) throw error(400, 'Invalid body: exclusive must be a boolean, workDate a YYYY-MM-DD date');

  const task = startTimer(actor.userId, id, parsed.data.workDate, parsed.data.exclusive);
  if (!task) throw error(404, 'Task not found');
  return json(task);
};
