import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { updateTask, deleteTask, LabelConflictError } from '$lib/server/taskService';

const patchSchema = z
  .object({
    // Identity fields — they belong to the task, not to one day.
    label: z.string().trim().min(1).max(200).optional(),
    description: z.string().trim().max(2000).nullable().optional(),
    code: z.string().trim().max(100).nullable().optional(),
    link: z.string().url().max(2000).nullable().optional(),
    notes: z.string().max(10000).nullable().optional(),
    tags: z.array(z.string().trim().min(1).max(50)).max(50).nullable().optional(),
    // Day fields — they apply to `workDate` (default today).
    workDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
    elapsed_seconds: z.number().int().min(0).optional(),
    done: z.boolean().optional(),
    status: z.string().trim().max(50).optional(),
    is_completed: z.boolean().optional(),
    is_cancelled: z.boolean().optional(),
    is_deleted: z.boolean().optional(),
    is_archived: z.boolean().optional(),
    is_pinned: z.boolean().optional(),
    is_important: z.boolean().optional()
  })
  .refine((v) => Object.keys(v).length > 0, { message: 'No fields to update' });

// PATCH /api/tasks/:id — update the task's content and/or the named day's state.
// Body: { label?, description?, code?, link?, notes?, tags? } → the task;
//       { elapsed_seconds?, done?, status?, is_*?, workDate? } → that day.
export const PATCH: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  const parsed = patchSchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, parsed.error.issues.map((i) => i.message).join('; '));

  // A rename onto an existing label is a conflict, not a server fault: answer 409
  // with the label so the caller can pick another name.
  let task;
  try {
    task = updateTask(
      actor.userId,
      id,
      {
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
      },
      parsed.data.workDate
    );
  } catch (err) {
    if (err instanceof LabelConflictError) throw error(409, err.message);
    throw err;
  }
  if (!task) throw error(404, 'Task not found');
  return json(task);
};

// DELETE /api/tasks/:id — delete the task; its days, comments and integrations go with it.
export const DELETE: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const id = Number(event.params.id);
  if (!Number.isInteger(id)) throw error(400, 'Invalid task id');

  if (!deleteTask(actor.userId, id)) throw error(404, 'Task not found');
  return new Response(null, { status: 204 });
};
