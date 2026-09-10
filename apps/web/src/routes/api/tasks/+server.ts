import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { listTasks, createTask } from '$lib/server/taskService';

// GET /api/tasks — list the caller's tasks with computed elapsed + total.
// Query: ?date=YYYY-MM-DD&archived=bool&done=bool&status=str&q=str&tag=str
export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');
  const date = event.url.searchParams.get('date');
  if (date !== null && !/^\d{4}-\d{2}-\d{2}$/.test(date)) {
    throw error(400, 'Invalid date; expected YYYY-MM-DD');
  }
  const archived = event.url.searchParams.get('archived');
  const done = event.url.searchParams.get('done');
  const status = event.url.searchParams.get('status');
  const q = event.url.searchParams.get('q');
  const tag = event.url.searchParams.get('tag');
  const parseBool = (v: string | null) => (v === 'true' ? true : v === 'false' ? false : undefined);
  return json(listTasks(actor.userId, {
    workDate: date ?? undefined,
    archived: parseBool(archived),
    done: parseBool(done),
    status: status ?? undefined,
    q: q ?? undefined,
    tag: tag ?? undefined,
  }));
};

const createSchema = z.object({
  label: z.string().trim().min(1).max(200),
  description: z.string().trim().max(2000).optional(),
  workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
  code: z.string().trim().max(100).nullable().optional(),
  link: z.string().url().max(2000).nullable().optional(),
  status: z.string().trim().max(50).optional(),
  notes: z.string().max(10000).nullable().optional(),
  tags: z.array(z.string().trim().min(1).max(50)).max(50).nullable().optional()
});

// POST /api/tasks — create a task. Body: { label, description?, workDate? }
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const parsed = createSchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, parsed.error.issues.map((i) => i.message).join('; '));

  const task = await createTask(actor.userId, {
    ...parsed.data,
    description: parsed.data.description ?? null
  });
  return json(task, {
    status: 201
  });
};
