// Domain layer for task/timer operations, scoped by user, on local SQLite.
// Used by the REST API (browser + AI agents). Pure timer math lives in `shared`.
import { and, asc, eq, ne } from 'drizzle-orm';
import { currentElapsedSeconds, buildMarkdownReport, buildCsvReport } from 'shared';
import { db, schema } from './db';
import { publish } from './events';
import * as jira from './jira';
import type { Task } from './db/schema';

const { tasks, taskComments, taskIntegrations, userSettings } = schema;

export type TimerMode = 'focus' | 'parallel';

export interface TaskDTO {
  id: number;
  label: string;
  description: string | null;
  code: string | null;
  link: string | null;
  status: string;
  notes: string | null;
  tags: string[] | null;
  position: number;
  isRunning: boolean;
  done: boolean;
  isCompleted: boolean;
  isCancelled: boolean;
  isDeleted: boolean;
  isArchived: boolean;
  isPinned: boolean;
  isImportant: boolean;
  totalTime: number;
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
    code: row.code ?? null,
    link: row.link ?? null,
    status: row.status,
    notes: row.notes ?? null,
    tags: row.tags ?? null,
    position: row.position,
    isRunning: row.isRunning,
    done: row.done,
    isCompleted: row.isCompleted,
    isCancelled: row.isCancelled,
    isDeleted: row.isDeleted,
    isArchived: row.isArchived,
    isPinned: row.isPinned,
    isImportant: row.isImportant,
    totalTime: row.totalTime,
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

export interface TaskCreate {
  label: string;
  description: string | null;
  code?: string | null;
  link?: string | null;
  status?: string;
  notes?: string | null;
  tags?: string[] | null;
}

export async function createTask(userId: string, input: TaskCreate): Promise<TaskDTO> {
  const { label, description } = input;
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
      code: input.code ?? null,
      description: finalDescription,
      link: input.link ?? null,
      status: input.status ?? 'todo',
      notes: input.notes ?? null,
      tags: input.tags ?? null,
      elapsedTime: 0,
      position: count,
      isRunning: false,
      createdAt: now,
      updatedAt: now
    })
    .returning()
    .get();
  publish(userId, { entity: 'task', taskId: row.id, action: 'create' });
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
  publish(userId, { entity: 'task', taskId, action: 'start' });
  void jira.onStart(updated);
  return toDTO(updated);
}

export function stopTimer(userId: string, taskId: number): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;
  const startMs = row.startTime ? row.startTime.getTime() : null;
  const delta = stopRow(userId, row);
  const updated = getOwnedRow(userId, taskId)!;
  publish(userId, { entity: 'task', taskId, action: 'stop' });
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
  publish(userId, { entity: 'task', taskId, action: 'reset' });
  return toDTO(updated);
}

export function resetAll(userId: string): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  db.update(tasks)
    .set({ isRunning: false, elapsedTime: 0, startTime: null, updatedAt: new Date() })
    .where(eq(tasks.userId, userId))
    .run();
  publish(userId, { entity: 'task', taskId: 0, action: 'reset_all' });
  return listTasks(userId);
}

export function listComments(userId: string, taskId: number) {
  if (!getOwnedRow(userId, taskId)) return null;
  return db.select().from(taskComments).where(eq(taskComments.taskId, taskId)).all();
}

export function addComment(userId: string, taskId: number, input: {
  subject?: string | null; summary?: string | null; branch?: string | null; pr?: string | null;
}) {
  if (!getOwnedRow(userId, taskId)) return null;
  const row = db.insert(taskComments).values({ taskId, ...input }).returning().get();
  publish(userId, { entity: 'comment', taskId });
  return row;
}

export function listIntegrations(userId: string, taskId: number) {
  if (!getOwnedRow(userId, taskId)) return null;
  return db.select().from(taskIntegrations).where(eq(taskIntegrations.taskId, taskId)).all();
}

export function upsertIntegration(userId: string, taskId: number, group: string, field: string, value: string | null) {
  if (!getOwnedRow(userId, taskId)) return null;
  const existing = db.select().from(taskIntegrations)
    .where(and(eq(taskIntegrations.taskId, taskId), eq(taskIntegrations.group, group), eq(taskIntegrations.field, field)))
    .get();
  const now = new Date();
  const row = existing
    ? db.update(taskIntegrations).set({ value, updatedAt: now }).where(eq(taskIntegrations.id, existing.id)).returning().get()
    : db.insert(taskIntegrations).values({ taskId, group, field, value, createdAt: now, updatedAt: now }).returning().get();
  publish(userId, { entity: 'integration', taskId });
  return row;
}

export function deleteTask(userId: string, taskId: number): boolean {
  const res = db
    .delete(tasks)
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .run();
  if (res.changes > 0) publish(userId, { entity: 'task', taskId, action: 'delete' });
  return res.changes > 0;
}

export interface TaskUpdate {
  label?: string;
  description?: string | null;
  elapsedSeconds?: number;
  done?: boolean;
  code?: string | null;
  link?: string | null;
  status?: string;
  notes?: string | null;
  tags?: string[] | null;
  isCompleted?: boolean;
  isCancelled?: boolean;
  isDeleted?: boolean;
  isArchived?: boolean;
  isPinned?: boolean;
  isImportant?: boolean;
}

export function updateTask(userId: string, taskId: number, changes: TaskUpdate): TaskDTO | null {
  const row = getOwnedRow(userId, taskId);
  if (!row) return null;

  const set: Partial<Task> = { updatedAt: new Date() };
  if (changes.label !== undefined) set.label = changes.label;
  if (changes.description !== undefined) set.description = changes.description;
  if (changes.code !== undefined) set.code = changes.code;
  if (changes.link !== undefined) set.link = changes.link;
  if (changes.status !== undefined) set.status = changes.status;
  if (changes.notes !== undefined) set.notes = changes.notes;
  if (changes.tags !== undefined) set.tags = changes.tags;
  if (changes.isCompleted !== undefined) set.isCompleted = changes.isCompleted;
  if (changes.isCancelled !== undefined) set.isCancelled = changes.isCancelled;
  if (changes.isDeleted !== undefined) set.isDeleted = changes.isDeleted;
  if (changes.isArchived !== undefined) set.isArchived = changes.isArchived;
  if (changes.isPinned !== undefined) set.isPinned = changes.isPinned;
  if (changes.isImportant !== undefined) set.isImportant = changes.isImportant;
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
  publish(userId, { entity: 'task', taskId, action: 'update' });
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
  publish(userId, { entity: 'task', taskId: 0, action: 'reorder' });
  return listTasks(userId);
}
