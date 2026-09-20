// Pure, synchronous create-task logic — accepts a Drizzle db instance so it can
// be tested against in-memory SQLite without SvelteKit env / module-level db.
//
// A task is an identity (one row per user + label) plus one row per day it is
// worked on. Creating a task finds or inserts the identity, then makes sure that
// day's row exists — so the same label on a new day is the same task with one
// more day, never a second task.
import { and, eq, sql } from 'drizzle-orm';
import type { BetterSQLite3Database } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';
import type { Task } from './db/schema';

const { tasks, taskDays } = schema;

/** One day's row, as stored. */
export type TaskDay = typeof taskDays.$inferSelect;

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

/** The task identity for (user, label), or undefined when the user has no such task. */
export function findIdentity(
  db: BetterSQLite3Database<typeof schema>,
  userId: string,
  label: string
): Task | undefined {
  return db
    .select()
    .from(tasks)
    .where(and(eq(tasks.userId, userId), eq(tasks.label, label)))
    .get();
}

/**
 * Insert a new task identity. The label is the identity, so the content fields —
 * description, code, link, notes, tags — live here and are shared by every day.
 */
export function insertIdentity(
  db: BetterSQLite3Database<typeof schema>,
  userId: string,
  input: CreateInput
): Task {
  const now = new Date();
  return db
    .insert(tasks)
    .values({
      userId,
      label: input.label,
      code: input.code ?? null,
      description: input.description,
      link: input.link ?? null,
      notes: input.notes ?? null,
      tags: input.tags ?? null,
      createdAt: now,
      updatedAt: now
    })
    .returning()
    .get();
}

/**
 * The day row for (task, date), created at the end of that day's list when
 * missing. `status` only applies to a row being created: an existing day is
 * returned untouched, so creating the same task again never resets its day.
 *
 * This is what "starting a task on a new day" calls: the identity already
 * exists, and the day row is the only thing that is new.
 */
export function ensureDayRow(
  db: BetterSQLite3Database<typeof schema>,
  userId: string,
  taskId: number,
  workDate: string,
  status?: string
): TaskDay {
  const existing = db
    .select({ day: taskDays })
    .from(taskDays)
    .innerJoin(tasks, eq(tasks.id, taskDays.taskId))
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId), eq(taskDays.workDate, workDate)))
    .get();
  if (existing) return existing.day;

  // `position` orders one day's list across the user's tasks, so it counts that
  // date's rows only.
  const posRow = db
    .select({ max: sql<number>`coalesce(max(${taskDays.position}), -1)` })
    .from(taskDays)
    .innerJoin(tasks, eq(tasks.id, taskDays.taskId))
    .where(and(eq(tasks.userId, userId), eq(taskDays.workDate, workDate)))
    .get();
  const now = new Date();
  return db
    .insert(taskDays)
    .values({
      taskId,
      workDate,
      status: status ?? 'todo',
      position: (posRow?.max ?? -1) + 1,
      createdAt: now,
      updatedAt: now
    })
    .returning()
    .get();
}
