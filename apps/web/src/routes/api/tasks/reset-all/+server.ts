import { json, type RequestHandler } from '@sveltejs/kit';
import { resolveActor, requireScope } from '$lib/server/actor';
import { resetAll } from '$lib/server/taskService';

// POST /api/tasks/reset-all — reset elapsed time to 0.
// Optional body: { workDate?: "YYYY-MM-DD" } — scoped to that day if given.
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');
  const body = await event.request.json().catch(() => null);
  const workDate = body?.workDate;
  if (workDate && !/^\d{4}-\d{2}-\d{2}$/.test(workDate)) {
    return json({ message: 'Invalid workDate; expected YYYY-MM-DD' }, { status: 400 });
  }
  return json(resetAll(actor.userId, workDate || undefined));
};
