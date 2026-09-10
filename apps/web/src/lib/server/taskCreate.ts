// Pure, synchronous create-task logic — accepts a Drizzle db instance so it can
// be tested against in-memory SQLite without SvelteKit env / module-level db.
import { and, desc, eq, sql } from 'drizzle-orm';
import type { BetterSQLite3Database } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';
import type { Task } from './db/schema';

const { tasks } = schema;

export interface CreateInput {
  label: string; // already trimmed
  workDate: string;
  description: string | null;
  code?: string | null;
  link?: string | null;
  status?: string;
  notes?: string | null;
  tags?: string[] | null;
}

/** Check for existing same-label-same-day row; if none, resolve description + position. */
export function resolveCreate(
  db: BetterSQLite3Database<typeof schema>,
  userId: string,
  input: CreateInput
): { existing: Task } | { existing: null; description: string | null; position: number } {
  const { label, workDate } = input;

  // 1. Same label + same day → return existing (TUI dedup).
  const existing = db
    .select()
    .from(tasks)
    .where(and(eq(tasks.userId, userId), eq(tasks.workDate, workDate), eq(tasks.label, label)))
    .get();
  if (existing) return { existing };

  // 2. Resolve description: caller's or copy from most recent same-label row.
  let description = input.description ?? null;
  if (!description || description.trim() === '') {
    const copied = db
      .select({ description: tasks.description })
      .from(tasks)
      .where(
        and(
          eq(tasks.userId, userId),
          eq(tasks.label, label),
          sql`${tasks.description} IS NOT NULL`,
          sql`${tasks.description} != ''`
        )
      )
      .orderBy(desc(tasks.workDate), desc(tasks.id))
      .limit(1)
      .get();
    if (copied?.description) description = copied.description;
  }

  // 3. Position = MAX(position) + 1 for this user+day.
  const posRow = db
    .select({ max: sql<number>`coalesce(max(${tasks.position}), -1)` })
    .from(tasks)
    .where(and(eq(tasks.userId, userId), eq(tasks.workDate, workDate)))
    .get();
  const position = (posRow?.max ?? -1) + 1;

  return { existing: null, description, position };
}

/** Insert a new task row. Returns the raw row. */
export function insertTask(
  db: BetterSQLite3Database<typeof schema>,
  userId: string,
  input: CreateInput,
  description: string | null,
  position: number
): Task {
  const now = new Date();
  return db
    .insert(tasks)
    .values({
      userId,
      label: input.label,
      workDate: input.workDate,
      code: input.code ?? null,
      description,
      link: input.link ?? null,
      status: input.status ?? 'todo',
      notes: input.notes ?? null,
      tags: input.tags ?? null,
      elapsedTime: 0,
      position,
      isRunning: false,
      createdAt: now,
      updatedAt: now
    })
    .returning()
    .get();
}
