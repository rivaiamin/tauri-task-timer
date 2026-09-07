import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { startTimer, stopTimer, listTasks } from '$lib/server/taskService';

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
      const task = startTimer(actor.userId, parsed.data.task_id, true);
      if (!task) throw error(404, 'Task not found');
      return json({ ok: true, task });
    }
    case 'waiting_user':
    case 'session_end': {
      // Stop all running timers for this user today.
      const { tasks } = listTasks(actor.userId);
      const running = tasks.filter((t) => t.isRunning);
      for (const t of running) {
        stopTimer(actor.userId, t.id);
      }
      return json({ ok: true, stopped: running.length });
    }
  }
};
