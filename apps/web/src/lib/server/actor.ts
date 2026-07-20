import { createHash } from 'node:crypto';
import { error, type RequestEvent } from '@sveltejs/kit';
import { eq } from 'drizzle-orm';
import { db, schema } from './db';

export interface Actor {
  userId: string;
  source: 'session' | 'apikey';
  scopes: string[];
}

const SESSION_SCOPES = ['tasks:read', 'tasks:write'];

/**
 * Resolve the acting user from either a logged-in session (the app's browser)
 * or an `Authorization: Bearer <key>` API key (AI agents / scripts).
 * Throws 401 if neither is present/valid.
 */
export async function resolveActor(event: RequestEvent): Promise<Actor> {
  // 1. Session cookie (populated by hooks.server.ts)
  if (event.locals.user) {
    return { userId: event.locals.user.id, source: 'session', scopes: SESSION_SCOPES };
  }

  // 2. API key
  const header = event.request.headers.get('authorization') ?? '';
  const match = header.match(/^Bearer\s+(.+)$/i);
  if (!match) throw error(401, 'Authentication required');

  const keyHash = createHash('sha256').update(match[1].trim()).digest('hex');
  const row = db.select().from(schema.apiKeys).where(eq(schema.apiKeys.keyHash, keyHash)).get();
  if (!row || row.revoked) throw error(401, 'Invalid or revoked API key');

  // Best-effort usage stamp.
  try {
    db.update(schema.apiKeys).set({ lastUsedAt: new Date() }).where(eq(schema.apiKeys.id, row.id)).run();
  } catch {
    // ignore
  }

  return { userId: row.userId, source: 'apikey', scopes: row.scopes };
}

export function requireScope(actor: Actor, scope: string): void {
  if (!actor.scopes.includes(scope)) {
    throw error(403, `API key is missing required scope: ${scope}`);
  }
}
