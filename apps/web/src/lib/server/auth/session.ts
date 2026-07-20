import { createHash, randomBytes } from 'node:crypto';
import type { Cookies } from '@sveltejs/kit';
import { eq } from 'drizzle-orm';
import { dev } from '$app/environment';
import { db, schema } from '../db';

export const SESSION_COOKIE = 'session';

const DAY_MS = 1000 * 60 * 60 * 24;
const SESSION_TTL_MS = 30 * DAY_MS;
const RENEW_THRESHOLD_MS = 15 * DAY_MS;

export interface SessionUser {
  id: string;
  email: string;
}
export interface SessionInfo {
  id: string;
  userId: string;
  expiresAt: Date;
}

function hashToken(token: string): string {
  return createHash('sha256').update(token).digest('hex');
}

export function generateSessionToken(): string {
  return randomBytes(24).toString('base64url');
}

export function createSession(userId: string): { token: string; expiresAt: Date } {
  const token = generateSessionToken();
  const id = hashToken(token);
  const expiresAt = new Date(Date.now() + SESSION_TTL_MS);
  db.insert(schema.sessions).values({ id, userId, expiresAt }).run();
  return { token, expiresAt };
}

export function validateSessionToken(
  token: string
): { session: SessionInfo | null; user: SessionUser | null } {
  const id = hashToken(token);
  const row = db.select().from(schema.sessions).where(eq(schema.sessions.id, id)).get();
  if (!row) return { session: null, user: null };

  if (Date.now() >= row.expiresAt.getTime()) {
    db.delete(schema.sessions).where(eq(schema.sessions.id, id)).run();
    return { session: null, user: null };
  }

  let expiresAt = row.expiresAt;
  // Sliding expiration: extend when close to expiry.
  if (Date.now() >= row.expiresAt.getTime() - RENEW_THRESHOLD_MS) {
    expiresAt = new Date(Date.now() + SESSION_TTL_MS);
    db.update(schema.sessions).set({ expiresAt }).where(eq(schema.sessions.id, id)).run();
  }

  const user = db
    .select({ id: schema.users.id, email: schema.users.email })
    .from(schema.users)
    .where(eq(schema.users.id, row.userId))
    .get();
  if (!user) return { session: null, user: null };

  return { session: { id: row.id, userId: row.userId, expiresAt }, user };
}

export function invalidateSession(id: string): void {
  db.delete(schema.sessions).where(eq(schema.sessions.id, id)).run();
}

export function setSessionCookie(cookies: Cookies, token: string, expiresAt: Date): void {
  cookies.set(SESSION_COOKIE, token, {
    path: '/',
    httpOnly: true,
    sameSite: 'lax',
    secure: !dev,
    expires: expiresAt
  });
}

export function deleteSessionCookie(cookies: Cookies): void {
  cookies.delete(SESSION_COOKIE, { path: '/' });
}
