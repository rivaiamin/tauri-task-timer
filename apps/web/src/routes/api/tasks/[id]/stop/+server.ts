import { json, error, type RequestHandler } from '@sveltejs/kit';
import { requireApiKey, requireScope } from '$lib/server/auth';
import { stopTimer } from '$lib/server/taskService';

// POST /api/tasks/:id/stop — stop a timer, accumulating elapsed time.
export const POST: RequestHandler = async (event) => {
  const actor = await requireApiKey(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const task = await stopTimer(actor.userId, id);
  if (!task) throw error(404, 'Task not found');
  return json(task);
};
