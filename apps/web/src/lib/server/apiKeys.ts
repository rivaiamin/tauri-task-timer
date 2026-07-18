import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { and, desc, eq } from 'drizzle-orm';
import { db, schema } from './db';

const { apiKeys } = schema;

export interface CreatedKey {
  id: string;
  key: string; // raw key — returned ONCE
  keyPrefix: string;
  name: string | null;
}

export function createApiKey(userId: string, name: string | null): CreatedKey {
  const raw = 'sk_live_' + randomBytes(24).toString('hex');
  const keyHash = createHash('sha256').update(raw).digest('hex');
  const keyPrefix = raw.slice(0, 16);
  const id = randomUUID();
  db.insert(apiKeys).values({ id, userId, keyHash, keyPrefix, name: name ?? null }).run();
  return { id, key: raw, keyPrefix, name: name ?? null };
}

export function listApiKeys(userId: string) {
  return db
    .select({
      id: apiKeys.id,
      keyPrefix: apiKeys.keyPrefix,
      name: apiKeys.name,
      revoked: apiKeys.revoked,
      lastUsedAt: apiKeys.lastUsedAt,
      createdAt: apiKeys.createdAt
    })
    .from(apiKeys)
    .where(eq(apiKeys.userId, userId))
    .orderBy(desc(apiKeys.createdAt))
    .all();
}

export function revokeApiKey(userId: string, id: string): boolean {
  const res = db
    .update(apiKeys)
    .set({ revoked: true })
    .where(and(eq(apiKeys.id, id), eq(apiKeys.userId, userId)))
    .run();
  return res.changes > 0;
}
