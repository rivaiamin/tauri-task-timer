import { json, error, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { resetTask } from '$lib/server/taskService';

// POST /api/tasks/:id/reset — reset a single task's elapsed time to 0.
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const task = resetTask(actor.userId, id);
  if (!task) throw error(404, 'Task not found');
  return json(task);
};
