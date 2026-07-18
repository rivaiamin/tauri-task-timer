import { createHash } from 'node:crypto';
import { error, type RequestEvent } from '@sveltejs/kit';
import { supabaseAdmin } from '$lib/supabaseServer';

export interface ApiActor {
  userId: string;
  keyId: string;
  scopes: string[];
}

/**
 * Authenticate an API request by its `Authorization: Bearer <key>` header.
 * Looks up the SHA-256 hash of the key in `api_keys`, returns the owning user,
 * and best-effort stamps `last_used_at`. Throws 401 on any failure.
 */
export async function requireApiKey(event: RequestEvent): Promise<ApiActor> {
  const header = event.request.headers.get('authorization') ?? '';
  const match = header.match(/^Bearer\s+(.+)$/i);
  if (!match) throw error(401, 'Missing or malformed Authorization header');

  const rawKey = match[1].trim();
  const keyHash = createHash('sha256').update(rawKey).digest('hex');

  const { data, error: dbError } = await supabaseAdmin
    .from('api_keys')
    .select('id, user_id, scopes, revoked')
    .eq('key_hash', keyHash)
    .maybeSingle();

  if (dbError) throw error(500, 'Failed to verify API key');
  if (!data || data.revoked) throw error(401, 'Invalid or revoked API key');

  // Best-effort usage stamp; never block the request on it.
  void supabaseAdmin
    .from('api_keys')
    .update({ last_used_at: new Date().toISOString() })
    .eq('id', data.id)
    .then(
      () => {},
      () => {}
    );

  return { userId: data.user_id, keyId: data.id, scopes: data.scopes ?? [] };
}

/** Throw 403 unless the actor holds the given scope. */
export function requireScope(actor: ApiActor, scope: string): void {
  if (!actor.scopes.includes(scope)) {
    throw error(403, `API key is missing required scope: ${scope}`);
  }
}
