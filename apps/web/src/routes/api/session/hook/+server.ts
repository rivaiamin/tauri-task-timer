import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { startTimer, stopTimer, listArchive } from '$lib/server/taskService';

const hookSchema = z.discriminatedUnion('event', [
  z.object({ event: z.literal('session_start'), task_id: z.number().int().positive() }),
  z.object({ event: z.literal('waiting_user') }),
  z.object({ event: z.literal('session_end') }),
]);

// POST /api/session/hook — agent session lifecycle events.
// Body: { event: "session_start", task_id } | { event: "waiting_user" } | { event: "session_end" }
export const POST: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const parsed = hookSchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, parsed.error.issues.map((i) => i.message).join('; '));

  const { event: hookEvent } = parsed.data;

  switch (hookEvent) {
    case 'session_start': {
      const task = startTimer(actor.userId, parsed.data.task_id, undefined, true);
      if (!task) throw error(404, 'Task not found');
      return json({ ok: true, task });
    }
    case 'waiting_user':
    case 'session_end': {
      // Stop every running timer, on any day: an agent session ending is not
      // scoped to today, and a run started yesterday and left going must stop too.
      const { tasks } = listArchive(actor.userId);
      const running = tasks.flatMap((t) =>
        (t.days ?? []).filter((d) => d.isRunning).map((d) => ({ taskId: t.id, workDate: d.workDate }))
      );
      for (const r of running) stopTimer(actor.userId, r.taskId, r.workDate);
      return json({ ok: true, stopped: running.length });
    }
  }
};
