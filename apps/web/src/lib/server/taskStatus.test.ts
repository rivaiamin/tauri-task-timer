// Port of the TUI's `auto_status_follows_the_rows_own_state` oracle
// (apps/tui/src/jira.rs). Same cases, same expected statuses: this is the web
// half of the parity contract, so a drift in either app fails a test.
//
// Status is per DAY, so these cases run against `task_days`: a task is an
// identity, and the timer/lifecycle state the status describes lives on the day.
import { describe, it, expect, beforeEach } from 'vitest';
import Database from 'better-sqlite3';
import { eq } from 'drizzle-orm';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import type { BetterSQLite3Database } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';
import { autoStatus, statusForRunState } from './taskStatus';

const { tasks, taskDays } = schema;

const DAY = '2026-09-21';

function setup(): BetterSQLite3Database<typeof schema> {
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
    INSERT INTO users VALUES ('u1', 'test@test.com', 'hash', 0);
  `);
	return drizzle(sqlite, { schema });
}

describe('statusForRunState', () => {
	it('names a status for every combination of the two flags', () => {
		expect(statusForRunState(true, false)).toBe('21');
		expect(statusForRunState(false, true)).toBe('31');
		expect(statusForRunState(false, false)).toBe('11');
		// A running task that is also flagged done still reads In Progress — the
		// live timer is the more specific state.
		expect(statusForRunState(true, true)).toBe('21');
	});
});

describe('autoStatus', () => {
	let db: BetterSQLite3Database<typeof schema>;
	/** task id → the id of its day row on DAY. */
	const dayIds: Record<number, number> = {};

	beforeEach(() => {
		db = setup();
		db.insert(tasks)
			.values([
				{ id: 1, userId: 'u1', label: 'A', createdAt: new Date(), updatedAt: new Date() },
				{ id: 2, userId: 'u1', label: 'B', createdAt: new Date(), updatedAt: new Date() }
			])
			.run();
		dayIds[1] = db
			.insert(taskDays)
			.values({ taskId: 1, workDate: DAY, status: '51', createdAt: new Date(), updatedAt: new Date() })
			.returning({ id: taskDays.id })
			.get().id;
		dayIds[2] = db
			.insert(taskDays)
			.values({ taskId: 2, workDate: DAY, status: 'todo', createdAt: new Date(), updatedAt: new Date() })
			.returning({ id: taskDays.id })
			.get().id;
	});

	function statusOf(taskId: number): string {
		return db
			.select({ status: taskDays.status })
			.from(taskDays)
			.where(eq(taskDays.id, dayIds[taskId]))
			.get()!.status;
	}

	function setState(taskId: number, state: { isRunning?: boolean; done?: boolean }): void {
		db.update(taskDays).set(state).where(eq(taskDays.id, dayIds[taskId])).run();
	}

	it('follows the row own state through start, stop, done, and un-done', () => {
		// Start: the layout change wins over whatever the row said (was Local OK).
		setState(1, { isRunning: true });
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('21');

		// Stop: back to To Do, so a finished run does not leave In Progress behind.
		setState(1, { isRunning: false });
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('11');

		// Done: Done, even straight from a legacy value.
		setState(2, { done: true });
		autoStatus(db, 'u1', 2, DAY);
		expect(statusOf(2)).toBe('31');

		// Un-done: back to To Do rather than staying on Done.
		setState(2, { done: false });
		autoStatus(db, 'u1', 2, DAY);
		expect(statusOf(2)).toBe('11');
	});

	it('is idempotent and never resurrects In Progress after a stop', () => {
		setState(1, { isRunning: true });
		autoStatus(db, 'u1', 1, DAY);
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('21');

		setState(1, { isRunning: false });
		autoStatus(db, 'u1', 1, DAY);
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('11');
	});

	it('keeps Done through a stop, because the state after the stop is still done', () => {
		setState(1, { isRunning: true, done: true });
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('21');

		setState(1, { isRunning: false });
		autoStatus(db, 'u1', 1, DAY);
		expect(statusOf(1)).toBe('31');
	});

	it('is scoped to the owning user, and a missing row is a no-op', () => {
		// Wrong user: nothing written, no throw.
		autoStatus(db, 'someone-else', 1, DAY);
		expect(statusOf(1)).toBe('51');

		// Missing row: nothing written, no throw.
		expect(() => autoStatus(db, 'u1', 999, DAY)).not.toThrow();

		// A date the task has no row on is also a no-op, not a throw.
		expect(() => autoStatus(db, 'u1', 1, '1999-01-01')).not.toThrow();
		expect(statusOf(1)).toBe('51');
	});

	/// The status belongs to the day it describes: moving one day's timer must
	/// leave the same task's other days exactly as they were.
	it('touches only the named day, not the task\u2019s other days', () => {
		const otherDayId = db
			.insert(taskDays)
			.values({ taskId: 1, workDate: '2026-09-22', status: '51', createdAt: new Date(), updatedAt: new Date() })
			.returning({ id: taskDays.id })
			.get().id;

		setState(1, { isRunning: true });
		autoStatus(db, 'u1', 1, DAY);

		expect(statusOf(1)).toBe('21');
		expect(
			db.select({ status: taskDays.status }).from(taskDays).where(eq(taskDays.id, otherDayId)).get()!.status
		).toBe('51');
	});
});
