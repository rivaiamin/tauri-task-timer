import { error, type RequestHandler } from '@sveltejs/kit';
import { resolveActor } from '$lib/server/actor';
import { revokeApiKey } from '$lib/server/apiKeys';

// DELETE /api/keys/:id — revoke a key (session only).
export const DELETE: RequestHandler = async (event) => {
  const actor = await resolveActor(event);
  if (actor.source !== 'session') throw error(403, 'Key management requires a logged-in session');

  const id = event.params.id;
  if (!id) throw error(400, 'Missing key id');
  if (!revokeApiKey(actor.userId, id)) throw error(404, 'Key not found');
  return new Response(null, { status: 204 });
};
