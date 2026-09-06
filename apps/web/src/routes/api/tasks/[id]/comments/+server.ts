import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { addComment, listComments } from '$lib/server/taskService';

const schema = z.object({
  subject: z.string().max(200).nullable().optional(),
  summary: z.string().max(10000).nullable().optional(),
  branch: z.string().max(500).nullable().optional(),
  pr: z.string().max(500).nullable().optional()
});

export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');
  const taskId = Number(event.params.id);
  if (!Number.isInteger(taskId)) throw error(400, 'Invalid task id');
  const result = listComments(actor.userId, taskId);
  if (!result) throw error(404, 'Task not found');
  return json(result);
};

export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');
  const taskId = Number(event.params.id);
  if (!Number.isInteger(taskId)) throw error(400, 'Invalid task id');
  const parsed = schema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, 'Invalid comment');
  const result = addComment(actor.userId, taskId, parsed.data);
  if (!result) throw error(404, 'Task not found');
  return json(result, { status: 201 });
};
