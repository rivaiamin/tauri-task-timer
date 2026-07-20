import { json, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { resetAll } from '$lib/server/taskService';

// POST /api/tasks/reset-all — reset every task's elapsed time to 0.
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');
  return json(resetAll(actor.userId));
};
