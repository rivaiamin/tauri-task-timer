import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { listTasks, createTask } from '$lib/server/taskService';

// GET /api/tasks — list the caller's tasks with computed elapsed + total.
export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');
  const date = event.url.searchParams.get('date');
  if (date !== null && !/^\d{4}-\d{2}-\d{2}$/.test(date)) {
    throw error(400, 'Invalid date; expected YYYY-MM-DD');
  }
  return json(listTasks(actor.userId, date ?? undefined));
};

const createSchema = z.object({
  label: z.string().trim().min(1).max(200),
  description: z.string().trim().max(2000).optional()
});

// POST /api/tasks — create a task. Body: { label, description? }
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const parsed = createSchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, parsed.error.issues.map((i) => i.message).join('; '));

  const task = await createTask(actor.userId, parsed.data.label, parsed.data.description ?? null);
  return json(task, {
    status: 201
  });
};
