import { randomUUID } from 'node:crypto';
import { eq } from 'drizzle-orm';
import { db, schema } from '../db';
import { hashPassword, verifyPassword } from './password';

export interface AuthedUser {
  id: string;
  email: string;
}

function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

export function getUserByEmail(email: string) {
  return db.select().from(schema.users).where(eq(schema.users.email, normalizeEmail(email))).get() ?? null;
}

/** Create a user. Throws 'EMAIL_TAKEN' if the email already exists. */
export async function createUser(email: string, password: string): Promise<AuthedUser> {
  const normalized = normalizeEmail(email);
  if (getUserByEmail(normalized)) {
    throw new Error('EMAIL_TAKEN');
  }
  const passwordHash = await hashPassword(password);
  const id = randomUUID();
  db.insert(schema.users).values({ id, email: normalized, passwordHash }).run();
  return { id, email: normalized };
}

/** Return the user if the email/password are valid, else null. */
export async function verifyUser(email: string, password: string): Promise<AuthedUser | null> {
  const user = getUserByEmail(email);
  if (!user) return null;
  const ok = await verifyPassword(user.passwordHash, password);
  return ok ? { id: user.id, email: user.email } : null;
}
