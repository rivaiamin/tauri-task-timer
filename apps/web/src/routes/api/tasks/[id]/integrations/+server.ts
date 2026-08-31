import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { listIntegrations, upsertIntegration } from '$lib/server/taskService';

const schema = z.object({
  group: z.string().trim().min(1).max(100),
  field: z.string().trim().min(1).max(100),
  value: z.string().max(10000).nullable()
});

export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');
  const taskId = Number(event.params.id);
  if (!Number.isInteger(taskId)) throw error(400, 'Invalid task id');
  const result = listIntegrations(actor.userId, taskId);
  if (!result) throw error(404, 'Task not found');
  return json(result);
};

export const PATCH: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');
  const taskId = Number(event.params.id);
  if (!Number.isInteger(taskId)) throw error(400, 'Invalid task id');
  const parsed = schema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, 'Invalid integration');
  const result = upsertIntegration(actor.userId, taskId, parsed.data.group, parsed.data.field, parsed.data.value);
  if (!result) throw error(404, 'Task not found');
  return json(result);
};
