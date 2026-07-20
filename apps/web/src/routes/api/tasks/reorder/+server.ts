import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { reorderTasks } from '$lib/server/taskService';

const bodySchema = z.object({ ids: z.array(z.number().int()).min(1) });

// POST /api/tasks/reorder — set positions to match the given id order.
// Body: { ids: [3, 1, 2] }
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const parsed = bodySchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, 'Body must be { ids: number[] }');

  return json(reorderTasks(actor.userId, parsed.data.ids));
};
