import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor, requireScope } from '$lib/server/actor';
import { getTimerMode, setTimerMode } from '$lib/server/taskService';

// GET /api/settings/timer-mode — read the caller's timer mode.
export const GET: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:read');
  return json({ timer_mode: getTimerMode(actor.userId) });
};

const bodySchema = z.object({ timer_mode: z.enum(['focus', 'parallel']) });

// PUT /api/settings/timer-mode — set it. Body: { timer_mode: "focus" | "parallel" }
export const PUT: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  requireScope(actor, 'tasks:write');

  const parsed = bodySchema.safeParse(await event.request.json().catch(() => null));
  if (!parsed.success) throw error(400, 'timer_mode must be "focus" or "parallel"');

  return json(setTimerMode(actor.userId, parsed.data.timer_mode));
};
