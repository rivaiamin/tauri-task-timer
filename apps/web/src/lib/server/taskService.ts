// Domain layer for task/timer operations, scoped by user, on local SQLite.
// Used by the REST API (browser + AI agents). Pure timer math lives in `shared`.
import { and, asc, eq, ne } from 'drizzle-orm';
import { currentElapsedSeconds, buildMarkdownReport, buildCsvReport } from 'shared';
import { db, schema } from './db';
import { publish } from './events';
import * as jira from './jira';
import type { Task } from './db/schema';

const { tasks, userSettings } = schema;

export type TimerMode = 'focus' | 'parallel';

export interface TaskDTO {
  id: number;
  label: string;
  description: string | null;
  position: number;
  isRunning: boolean;
  done: boolean;
  startTime: number | null; // epoch ms
  elapsedSeconds: number; // stored (accumulated)
  currentElapsedSeconds: number; // stored + live
}

function toDTO(row: Task): TaskDTO {
  const startTimeMs = row.startTime ? row.startTime.getTime() : null;
  return {
    id: row.id,
    label: row.label,
    description: row.description ?? null,
    position: row.position,
    isRunning: row.isRunning,
    done: row.done,
    startTime: startTimeMs,
    elapsedSeconds: row.elapsedTime,
    currentElapsedSeconds: currentElapsedSeconds(row.elapsedTime, row.isRunning, startTimeMs)
  };
}

function getOwnedRow(userId: string, taskId: number): Task | undefined {
  return db
    .select()
    .from(tasks)
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .get();
}

export function getTimerMode(userId: string): TimerMode {
  const row = db
    .select({ mode: userSettings.timerMode })
    .from(userSettings)
    .where(eq(userSettings.userId, userId))
    .get();
  return (row?.mode as TimerMode) ?? 'focus';
}

export function setTimerMode(userId: string, mode: TimerMode): { timer_mode: TimerMode } {
  db.insert(userSettings)
    .values({ userId, timerMode: mode, updatedAt: new Date() })
    .onConflictDoUpdate({ target: userSettings.userId, set: { timerMode: mode, updatedAt: new Date() } })
    .run();
  return { timer_mode: mode };
}

export function listTasks(
  userId: string,
  workDate?: string
): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const where = workDate
    ? and(eq(tasks.userId, userId), eq(tasks.workDate, workDate))
    : eq(tasks.userId, userId);
  const rows = db.select().from(tasks).where(where).orderBy(asc(tasks.position)).all();
  const dtos = rows.map(toDTO);
  const total = dtos.reduce((sum, t) => sum + t.currentElapsedSeconds, 0);
  return { tasks: dtos, totalElapsedSeconds: total };
}

export async function createTask(userId: string, label: string, description: string | null): Promise<TaskDTO> {
  let finalDescription = description ?? null;
  try {
    finalDescription = await jira.onCreate(label, finalDescription);
  } catch (err) {
    console.error('[taskService] jira.onCreate error:', err instanceof Error ? err.message : err);
  }

  const count = db.select().from(tasks).where(eq(tasks.userId, userId)).all().length;
  const now = new Date();
  const workDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  const row = db
    .insert(tasks)
    .values({
      userId,
      label,
      workDate,
      description: finalDescription,
      elapsedTime: 0,
      position: count,
      isRunning: false,
      createdAt: now,
      updatedAt: now
    })
    .returning()
    .get();
  publish(userId);
  return toDTO(row);
}

// Stop one running row; returns the run's elapsed delta (seconds) for worklogging.
function stopRow(userId: string, row: Task): number {
  const elapsed = currentElapsedSeconds(
    row.elapsedTime,
    row.isRunning,
    row.startTime ? row.startTime.getTime() : null
  );
  db.update(tasks)
    .set({ isRunning: false, elapsedTime: elapsed, startTime: null, updatedAt: new Date() })
    .where(and(eq(tasks.id, row.id), eq(tasks.userId, userId)))
    .run();
  return elapsed - row.elapsedTime;
}

export function startTimer(userId: string, taskId: number, exclusive?: boolean): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;

  const isExclusive = exclusive ?? getTimerMode(userId) === 'focus';
  if (isExclusive) {
    const running = db
      .select()
      .from(tasks)
      .where(and(eq(tasks.userId, userId), eq(tasks.isRunning, true), ne(tasks.id, taskId)))
      .all();
    for (const r of running) {
      const delta = stopRow(userId, r);
      // Focus switch: log the interrupted run and send the issue back to To Do.
      void jira.onSwitchStop(r, delta, r.startTime ? r.startTime.getTime() : null);
    }
  }

  const updated = db
    .update(tasks)
    .set({ isRunning: true, startTime: new Date(), updatedAt: new Date() })
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .returning()
    .get();
  publish(userId);
  void jira.onStart(updated);
  return toDTO(updated);
}

export function stopTimer(userId: string, taskId: number): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;
  const startMs = row.startTime ? row.startTime.getTime() : null;
  const delta = stopRow(userId, row);
  const updated = getOwnedRow(userId, taskId)!;
  publish(userId);
  void jira.onStop(row, delta, startMs); // worklog only; keep the issue's status
  return toDTO(updated);
}

export function resetTask(userId: string, taskId: number): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;
  const updated = db
    .update(tasks)
    .set({ isRunning: false, elapsedTime: 0, startTime: null, updatedAt: new Date() })
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .returning()
    .get();
  publish(userId);
  return toDTO(updated);
}

export function resetAll(userId: string): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  db.update(tasks)
    .set({ isRunning: false, elapsedTime: 0, startTime: null, updatedAt: new Date() })
    .where(eq(tasks.userId, userId))
    .run();
  publish(userId);
  return listTasks(userId);
}

export function deleteTask(userId: string, taskId: number): boolean {
  const res = db
    .delete(tasks)
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .run();
  if (res.changes > 0) publish(userId);
  return res.changes > 0;
}

export interface TaskUpdate {
  label?: string;
  description?: string | null;
  elapsedSeconds?: number;
  done?: boolean;
}

export function updateTask(userId: string, taskId: number, changes: TaskUpdate): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;

  const set: Partial<Task> = { updatedAt: new Date() };
  if (changes.label !== undefined) set.label = changes.label;
  if (changes.description !== undefined) set.description = changes.description;
  if (changes.elapsedSeconds !== undefined) {
    set.elapsedTime = changes.elapsedSeconds;
    // If running, rebase the current run so live time continues from the new value.
    if (row.isRunning) set.startTime = new Date();
  }

  // Marking a task done (false→true): stop any live run and record its delta so the
  // JIRA hook can worklog it, then move the issue to Cek lokal.
  let doneDelta = 0;
  let doneStartMs: number | null = null;
  const becameDone = changes.done !== undefined && changes.done !== row.done;
  if (changes.done !== undefined) set.done = changes.done;
  if (becameDone && changes.done && row.isRunning) {
    doneStartMs = row.startTime ? row.startTime.getTime() : null;
    const elapsed = currentElapsedSeconds(row.elapsedTime, row.isRunning, doneStartMs);
    doneDelta = elapsed - row.elapsedTime;
    set.isRunning = false;
    set.elapsedTime = elapsed;
    set.startTime = null;
  }

  const updated = db
    .update(tasks)
    .set(set)
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .returning()
    .get();
  publish(userId);
  if (becameDone && changes.done) void jira.onDone(row, doneDelta, doneStartMs);
  return toDTO(updated);
}

export type ReportFormat = 'markdown' | 'csv';

/** Build a Daily Report over the user's current elapsed times. */
export function buildReport(
  userId: string,
  format: ReportFormat,
  dateISO: string
): { contentType: string; body: string } {
  const { tasks: dtos } = listTasks(userId);
  const reportTasks = dtos.map((t) => ({
    label: t.label,
    description: t.description,
    elapsedSeconds: t.currentElapsedSeconds
  }));
  if (format === 'csv') {
    return { contentType: 'text/csv; charset=utf-8', body: buildCsvReport(reportTasks) };
  }
  return { contentType: 'text/markdown; charset=utf-8', body: buildMarkdownReport(reportTasks, dateISO) };
}

/** Set positions to match the given id order (only the user's own tasks). */
export function reorderTasks(userId: string, orderedIds: number[]): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const owned = db.select({ id: tasks.id }).from(tasks).where(eq(tasks.userId, userId)).all();
  const ownedIds = new Set(owned.map((r) => r.id));

  db.transaction((tx) => {
    let pos = 0;
    for (const id of orderedIds) {
      if (!ownedIds.has(id)) continue;
      tx.update(tasks)
        .set({ position: pos, updatedAt: new Date() })
        .where(and(eq(tasks.id, id), eq(tasks.userId, userId)))
        .run();
      pos++;
    }
  });
  publish(userId);
  return listTasks(userId);
}
