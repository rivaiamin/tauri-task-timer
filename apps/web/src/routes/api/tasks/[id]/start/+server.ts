import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { requireApiKey, requireScope } from '$lib/server/auth';
import { startTimer } from '$lib/server/taskService';

const bodySchema = z.object({ exclusive: z.boolean().optional() });

// POST /api/tasks/:id/start — start a timer.
// Body: { exclusive? } — omit to use the user's timer mode (focus => exclusive).
export const POST: RequestHandler = async (event) => {
  const actor = await requireApiKey(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const parsed = bodySchema.safeParse(await event.request.json().catch(() => ({})));
  if (!parsed.success) throw error(400, 'Invalid body: exclusive must be a boolean');

  const task = await startTimer(actor.userId, id, parsed.data.exclusive);
  if (!task) throw error(404, 'Task not found');
  return json(task);
};
