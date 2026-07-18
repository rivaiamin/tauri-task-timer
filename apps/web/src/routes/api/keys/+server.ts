import { json, error, type RequestHandler } from '@sveltejs/kit';
import { z } from 'zod';
import { resolveActor } from '$lib/server/actor';
import { createApiKey, listApiKeys } from '$lib/server/apiKeys';

// Key management requires a logged-in session (an API key cannot mint more keys).
async function requireSession(event: Parameters<RequestHandler>[0]) {
  const actor = await resolveActor(event);
  if (actor.source !== 'session') throw error(403, 'Key management requires a logged-in session');
  return actor;
}

// GET /api/keys — list the caller's keys (never returns hashes or raw keys).
export const GET: RequestHandler = async (event) => {
  const actor = await requireSession(event);
  return json({ keys: listApiKeys(actor.userId) });
};

const createSchema = z.object({ name: z.string().trim().max(100).optional() });

// POST /api/keys — mint a new key. Returns the raw key ONCE. Body: { name? }
export const POST: RequestHandler = async (event) => {
  const actor = await requireSession(event);
  const parsed = createSchema.safeParse(await event.request.json().catch(() => ({})));
  if (!parsed.success) throw error(400, 'Invalid name');
  return json(createApiKey(actor.userId, parsed.data.name ?? null), { status: 201 });
};
