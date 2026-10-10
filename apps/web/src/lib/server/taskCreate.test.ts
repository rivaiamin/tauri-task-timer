// The create path: a task is an identity (one row per user + label) plus one row
// per day it is worked. These cases pin the rule the whole change exists for —
// the same label on a new day is the SAME task with one more day, never a second
// task — and that each day keeps its own recorded time.
import { describe, it, expect, beforeEach } from 'vitest';
import Database from 'better-sqlite3';
import { eq } from 'drizzle-orm';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';
import { findIdentity, insertIdentity, ensureDayRow } from './taskCreate';

const { tasks, taskDays } = schema;

function setup() {
  const sqlite = new Database(':memory:');
  sqlite.pragma('foreign_keys = ON');
  sqlite.exec(`
    CREATE TABLE users (
      id TEXT PRIMARY KEY,
      email TEXT NOT NULL UNIQUE,
      password_hash TEXT NOT NULL,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE tasks (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      label TEXT NOT NULL,
      code TEXT,
      description TEXT,
      link TEXT,
      notes TEXT,
      tags TEXT,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
    CREATE TABLE task_days (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
      work_date TEXT NOT NULL DEFAULT (date('now')),
      status TEXT NOT NULL DEFAULT 'todo',
      elapsed_time INTEGER NOT NULL DEFAULT 0,
      total_time INTEGER NOT NULL DEFAULT 0,
      position INTEGER NOT NULL DEFAULT 0,
      is_running INTEGER NOT NULL DEFAULT 0,
      done INTEGER NOT NULL DEFAULT 0,
      is_completed INTEGER NOT NULL DEFAULT 0,
      is_cancelled INTEGER NOT NULL DEFAULT 0,
      is_deleted INTEGER NOT NULL DEFAULT 0,
      is_archived INTEGER NOT NULL DEFAULT 0,
      is_pinned INTEGER NOT NULL DEFAULT 0,
      is_important INTEGER NOT NULL DEFAULT 0,
      start_time INTEGER,
      end_time INTEGER,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
    INSERT INTO users VALUES ('u1', 'test@test.com', 'hash', 0);
  `);
  return drizzle(sqlite, { schema });
}

type Db = ReturnType<typeof setup>;

/** The create path, as `createTask` drives it: find or insert the identity, then that day's row. */
function create(db: Db, userId: string, workDate: string, label: string, description?: string) {
  const input = { label: label.trim(), workDate, description: description ?? null };
  const identity = findIdentity(db, userId, input.label) ?? insertIdentity(db, userId, input);
  return { identity, day: ensureDayRow(db, userId, identity.id, workDate) };
}

describe('identity + day creation', () => {
  let db: Db;

  beforeEach(() => {
    db = setup();
  });

  it('the same label on a new day is the same task with its own day row', () => {
    const first = create(db, 'u1', '2026-08-30', 'US-1', 'first');
    const second = create(db, 'u1', '2026-08-31', 'US-1');

    // One task, not two.
    expect(second.identity.id).toBe(first.identity.id);
    expect(db.select().from(tasks).all()).toHaveLength(1);

    // Two days, each its own row.
    expect(second.day.id).not.toBe(first.day.id);
    expect(second.day.workDate).toBe('2026-08-31');
    const days = db.select().from(taskDays).all();
    expect(days).toHaveLength(2);

    // The description lives on the task, so the new day shares it.
    expect(second.identity.description).toBe('first');
  });

  it('creating the same label on the same day returns that day untouched', () => {
    const first = create(db, 'u1', '2026-08-30', 'US-1', 'first');
    db.update(taskDays).set({ elapsedTime: 3600 }).where(eq(taskDays.id, first.day.id)).run();

    const again = create(db, 'u1', '2026-08-30', 'US-1', 'ignored');

    expect(again.identity.id).toBe(first.identity.id);
    expect(again.day.id).toBe(first.day.id);
    // The existing day keeps its recorded time rather than being reset.
    expect(again.day.elapsedTime).toBe(3600);
    expect(db.select().from(taskDays).all()).toHaveLength(1);
  });

  it('each day keeps its own elapsed time', () => {
    const a = create(db, 'u1', '2026-08-30', 'US-1');
    const b = create(db, 'u1', '2026-08-31', 'US-1');
    db.update(taskDays).set({ elapsedTime: 3600 }).where(eq(taskDays.id, a.day.id)).run();
    db.update(taskDays).set({ elapsedTime: 1800 }).where(eq(taskDays.id, b.day.id)).run();

    const days = db.select().from(taskDays).all();
    expect(days.map((d) => d.elapsedTime).sort()).toEqual([1800, 3600]);
  });

  it('position is per day across tasks', () => {
    const a = create(db, 'u1', '2026-08-28', 'A');
    const b = create(db, 'u1', '2026-08-28', 'B');
    const c = create(db, 'u1', '2026-08-29', 'C');
    expect(a.day.position).toBe(0);
    expect(b.day.position).toBe(1);
    // C is on a different day — that day's ordering starts at 0.
    expect(c.day.position).toBe(0);
  });

  it('the same label for a different user is a different task', () => {
    db.insert(schema.users).values({ id: 'u2', email: 'b@b.c', passwordHash: 'x', createdAt: new Date() }).run();
    const mine = create(db, 'u1', '2026-08-30', 'US-1');
    const theirs = create(db, 'u2', '2026-08-30', 'US-1');
    expect(theirs.identity.id).not.toBe(mine.identity.id);
    expect(db.select().from(tasks).all()).toHaveLength(2);
  });

  it('a second day for a task gets its own position, not a duplicate', () => {
    const a = create(db, 'u1', '2026-08-28', 'A');
    const b = create(db, 'u1', '2026-08-28', 'B');
    // Re-creating A on the next day must not reuse B's position slot.
    const a2 = create(db, 'u1', '2026-08-29', 'A');
    expect(a2.day.position).toBe(0);
    expect(a.day.position).toBe(0);
    expect(b.day.position).toBe(1);
  });
});
