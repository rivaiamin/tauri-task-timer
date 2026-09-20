// Pure, synchronous status-lifecycle logic — accepts a Drizzle db instance so it
// can be tested against in-memory SQLite without SvelteKit env / module-level db.
//
// Port of `apps/tui/src/jira.rs` `status_for_run_state` + `auto_status`. The TUI
// is the reference: a layout change (start / stop / reset / done toggle) owns the
// stored status, so the list shows what the timer is doing without the operator
// editing the row. An explicit status edit does NOT call this — see taskService.
//
// Status is per DAY: the timer, the ordering and the lifecycle flags all live on
// the day row, so `autoStatus` reads and writes that row, not the identity.
import { and, eq } from 'drizzle-orm';
import type { BetterSQLite3Database } from 'drizzle-orm/better-sqlite3';
import * as schema from './db/schema';

const { tasks, taskDays } = schema;

/**
 * The status a task's own state implies, as a catalog id:
 * running → In Progress (`21`), done → Done (`31`), stopped and unfinished → To Do (`11`).
 *
 * Total on purpose: every combination of the two flags names a status, so a stop
 * cannot leave the row reading In Progress with no timer behind it.
 */
export function statusForRunState(isRunning: boolean, isDone: boolean): string {
	if (isRunning) return '21';
	if (isDone) return '31';
	return '11';
}

/**
 * Overwrite a day row's stored status with the one its own state now implies.
 *
 * Reads the state back from the day row rather than taking it from the caller, so
 * a caller holding a snapshot from before its own write cannot apply a stale
 * flag. No-op only when the row is already correct, so every caller can apply it
 * unconditionally after mutating the timer. A missing row is a no-op, not an
 * error — including a task that has no row on that date at all.
 */
export function autoStatus(
	db: BetterSQLite3Database<typeof schema>,
	userId: string,
	taskId: number,
	workDate: string
): void {
	const row = db
		.select({ id: taskDays.id, isRunning: taskDays.isRunning, done: taskDays.done, status: taskDays.status })
		.from(taskDays)
		.innerJoin(tasks, eq(tasks.id, taskDays.taskId))
		.where(and(eq(tasks.id, taskId), eq(tasks.userId, userId), eq(taskDays.workDate, workDate)))
		.get();
	if (!row) return;

	const status = statusForRunState(row.isRunning, row.done);
	if (row.status === status) return;

	db.update(taskDays)
		.set({ status, updatedAt: new Date() })
		.where(eq(taskDays.id, row.id))
		.run();
}
