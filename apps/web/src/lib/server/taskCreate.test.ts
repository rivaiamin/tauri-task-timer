import { describe, it, expect, beforeEach } from 'vitest';
import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';
import { resolveCreate, insertTask } from './taskCreate';

const { tasks } = schema;

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
      work_date TEXT NOT NULL DEFAULT (date('now')),
      description TEXT,
      code TEXT,
      link TEXT,
      status TEXT NOT NULL DEFAULT 'todo',
      notes TEXT,
      tags TEXT,
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
    CREATE UNIQUE INDEX idx_tasks_user_date_label ON tasks (user_id, work_date, label);
    INSERT INTO users VALUES ('u1', 'test@test.com', 'hash', 0);
  `);
  return drizzle(sqlite, { schema });
}

function create(db: ReturnType<typeof setup>, userId: string, workDate: string, label: string, description?: string) {
  const input = { label: label.trim(), workDate, description: description ?? null };
  const resolved = resolveCreate(db, userId, input);
  if (resolved.existing) return resolved.existing;
  return insertTask(db, userId, input, resolved.description, resolved.position);
}

describe('resolveCreate + insertTask', () => {
  let db: ReturnType<typeof setup>;

  beforeEach(() => {
    db = setup();
  });

  it('create_same_label_same_day_returns_existing', () => {
    const a = create(db, 'u1', '2026-08-30', 'US-1', 'first');
    const b = create(db, 'u1', '2026-08-30', 'US-1', 'ignored');
    expect(b.id).toBe(a.id);
    expect(b.description).toBe('first');
    // Only one row in the table
    const all = db.select().from(tasks).all();
    expect(all).toHaveLength(1);
  });

  it('create_same_label_new_day_copies_description', () => {
    create(db, 'u1', '2026-08-29', 'US-1', 'from yesterday');
    const b = create(db, 'u1', '2026-08-30', 'US-1');
    expect(b.description).toBe('from yesterday');
    expect(b.workDate).toBe('2026-08-30');
  });

  it('create_same_label_new_day_keeps_caller_description', () => {
    create(db, 'u1', '2026-08-29', 'US-1', 'from yesterday');
    const b = create(db, 'u1', '2026-08-30', 'US-1', 'explicit today');
    expect(b.description).toBe('explicit today');
  });

  it('create_position_is_per_day', () => {
    const a = create(db, 'u1', '2026-08-28', 'A');
    const b = create(db, 'u1', '2026-08-28', 'B');
    const c = create(db, 'u1', '2026-08-29', 'C');
    expect(a.position).toBe(0);
    expect(b.position).toBe(1);
    // C is on a different day — position resets to 0
    expect(c.position).toBe(0);
  });
});
