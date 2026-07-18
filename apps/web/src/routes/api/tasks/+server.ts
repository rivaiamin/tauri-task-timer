import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { requireApiKey, requireScope } from '$lib/server/auth';
import { listTasks, createTask } from '$lib/server/taskService';

// GET /api/tasks — list the caller's tasks with computed elapsed + total.
export const GET: RequestHandler = async (event) => {
  const actor = await requireApiKey(event);
  requireScope(actor, 'tasks:read');
  return json(await listTasks(actor.userId));
};

const createSchema = z.object({
  label: z.string().trim().min(1).max(200),
  description: z.string().trim().max(2000).optional()
});

// POST /api/tasks — create a task. Body: { label, description? }
export const POST: RequestHandler = async (event) => {
  const actor = await requireApiKey(event);
  requireScope(actor, 'tasks:write');

  const body = await event.request.json().catch(() => null);
  const parsed = createSchema.safeParse(body);
  if (!parsed.success) {
    throw error(400, parsed.error.issues.map((i) => i.message).join('; '));
  }

  const task = await createTask(actor.userId, parsed.data.label, parsed.data.description ?? null);
  return json(task, { status: 201 });
};
