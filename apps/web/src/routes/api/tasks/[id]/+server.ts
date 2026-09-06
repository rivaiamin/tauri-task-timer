import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { updateTask, deleteTask } from '$lib/server/taskService';

const patchSchema = z
  .object({
    label: z.string().trim().min(1).max(200).optional(),
    description: z.string().trim().max(2000).nullable().optional(),
    elapsed_seconds: z.number().int().min(0).optional(),
    done: z.boolean().optional(),
    code: z.string().trim().max(100).nullable().optional(),
    link: z.string().url().max(2000).nullable().optional(),
    status: z.string().trim().max(50).optional(),
    notes: z.string().max(10000).nullable().optional(),
    tags: z.array(z.string().trim().min(1).max(50)).max(50).nullable().optional(),
    is_completed: z.boolean().optional(),
    is_cancelled: z.boolean().optional(),
    is_deleted: z.boolean().optional(),
    is_archived: z.boolean().optional(),
    is_pinned: z.boolean().optional(),
    is_important: z.boolean().optional()
  })
  .refine((v) => Object.keys(v).length > 0, { message: 'No fields to update' });

// PATCH /api/tasks/:id — update label / description / elapsed_seconds.
export const PATCH: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const parsed = patchSchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, parsed.error.issues.map((i) => i.message).join('; '));

  const task = updateTask(actor.userId, id, {
    label: parsed.data.label,
    description: parsed.data.description,
    elapsedSeconds: parsed.data.elapsed_seconds,
    done: parsed.data.done,
    code: parsed.data.code,
    link: parsed.data.link,
    status: parsed.data.status,
    notes: parsed.data.notes,
    tags: parsed.data.tags,
    isCompleted: parsed.data.is_completed,
    isCancelled: parsed.data.is_cancelled,
    isDeleted: parsed.data.is_deleted,
    isArchived: parsed.data.is_archived,
    isPinned: parsed.data.is_pinned,
    isImportant: parsed.data.is_important
  });
  if (!task) throw error(404, 'Task not found');
  return json(task);
};

// DELETE /api/tasks/:id
export const DELETE: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  if (!deleteTask(actor.userId, id)) throw error(404, 'Task not found');
  return new Response(null, { status: 204 });
};
