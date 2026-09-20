// Domain layer for task/timer operations, scoped by user, on local SQLite.
// Used by the REST API (browser + AI agents). Pure timer math lives in `shared`.
//
// A task is an IDENTITY (one row per user + label, in `tasks`) plus one DAY ROW
// per date it was worked (in `task_days`). The identity id stays the public
// handle: every route still addresses `/api/tasks/:id/*` by task id, and a
// day-scoped query flattens the viewed day onto the task so the daily UI keeps
// reading `task.elapsedSeconds` / `task.isRunning` / `task.status` as before.
import { and, asc, desc, eq, inArray, like, ne } from 'drizzle-orm';
import { currentElapsedSeconds, buildMarkdownReport, buildCsvReport } from 'shared';
import { todayISO } from '$lib/dates';
import { ensureDayRow, findIdentity, insertIdentity } from './taskCreate';
import { autoStatus } from './taskStatus';
import { db, schema } from './db';
import { publish } from './events';
import * as jira from './jira';
import type { Task } from './db/schema';

const { tasks, taskDays, taskComments, taskIntegrations, userSettings } = schema;

type TaskDay = typeof taskDays.$inferSelect;

export type TimerMode = 'focus' | 'parallel';

/** One day of work, on its own. Only the archive exposes these. */
export interface TaskDayDTO {
  id: number; // task_days.id — the day row's own id
  workDate: string; // YYYY-MM-DD
  status: string;
  elapsedSeconds: number; // stored accumulated
  currentElapsedSeconds: number; // stored + live
  totalTime: number;
  position: number;
  isRunning: boolean;
  startTime: number | null; // epoch ms
  done: boolean;
  isCompleted: boolean;
  isCancelled: boolean;
  isDeleted: boolean;
  isArchived: boolean;
  isPinned: boolean;
  isImportant: boolean;
}

export interface TaskDTO {
  id: number; // TASK id (identity) — the public handle for /api/tasks/:id/*
  label: string;
  description: string | null;
  code: string | null;
  link: string | null;
  notes: string | null;
  tags: string[] | null;
  // The day being viewed, flattened onto the task.
  workDate: string | null; // the viewed day; null on an archive query
  dayId: number | null; // task_days.id of the viewed day
  status: string;
  elapsedSeconds: number;
  currentElapsedSeconds: number;
  totalTime: number;
  position: number;
  isRunning: boolean;
  startTime: number | null;
  done: boolean;
  isCompleted: boolean;
  isCancelled: boolean;
  isDeleted: boolean;
  isArchived: boolean;
  isPinned: boolean;
  isImportant: boolean;
  days?: TaskDayDTO[]; // archive only: every day, work_date ASC
}

function toDayDTO(row: TaskDay): TaskDayDTO {
  const startMs = row.startTime ? row.startTime.getTime() : null;
  return {
    id: row.id,
    workDate: row.workDate,
    status: row.status,
    elapsedSeconds: row.elapsedTime,
    currentElapsedSeconds: currentElapsedSeconds(row.elapsedTime, row.isRunning, startMs),
    totalTime: row.totalTime,
    position: row.position,
    isRunning: row.isRunning,
    startTime: startMs,
    done: row.done,
    isCompleted: row.isCompleted,
    isCancelled: row.isCancelled,
    isDeleted: row.isDeleted,
    isArchived: row.isArchived,
    isPinned: row.isPinned,
    isImportant: row.isImportant
  };
}

/** Flatten one day row onto its task identity. A task with no row for the viewed day reads zeroed. */
function toDTO(task: Task, day: TaskDay | null | undefined, days?: TaskDayDTO[]): TaskDTO {
  const startMs = day?.startTime ? day.startTime.getTime() : null;
  const dto: TaskDTO = {
    id: task.id,
    label: task.label,
    description: task.description ?? null,
    code: task.code ?? null,
    link: task.link ?? null,
    notes: task.notes ?? null,
    tags: task.tags ?? null,
    workDate: day?.workDate ?? null,
    dayId: day?.id ?? null,
    status: day?.status ?? 'todo',
    elapsedSeconds: day?.elapsedTime ?? 0,
    currentElapsedSeconds: day
      ? currentElapsedSeconds(day.elapsedTime, day.isRunning, startMs)
      : 0,
    totalTime: day?.totalTime ?? 0,
    position: day?.position ?? 0,
    isRunning: day?.isRunning ?? false,
    startTime: startMs,
    done: day?.done ?? false,
    isCompleted: day?.isCompleted ?? false,
    isCancelled: day?.isCancelled ?? false,
    isDeleted: day?.isDeleted ?? false,
    isArchived: day?.isArchived ?? false,
    isPinned: day?.isPinned ?? false,
    isImportant: day?.isImportant ?? false
  };
  if (days) dto.days = days;
  return dto;
}

/** The caller's task identity, or undefined when it is not theirs (or not there). */
function getOwnedTask(userId: string, taskId: number): Task | undefined {
  return db
    .select()
    .from(tasks)
    .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
    .get();
}

/** Every identity the user owns, keyed by id — one query for the batch helpers. */
function ownedTasksById(userId: string): Map<number, Task> {
  const rows = db.select().from(tasks).where(eq(tasks.userId, userId)).all();
  return new Map(rows.map((r) => [r.id, r]));
}

/** One day row of a task. Ownership is the caller's job (it resolves the task first). */
function getDay(taskId: number, workDate: string): TaskDay | undefined {
  return db
    .select()
    .from(taskDays)
    .where(and(eq(taskDays.taskId, taskId), eq(taskDays.workDate, workDate)))
    .get();
}

/** The task's live run, whatever date it belongs to — a running timer is running. */
function getRunningDay(taskId: number): TaskDay | undefined {
  return db
    .select()
    .from(taskDays)
    .where(and(eq(taskDays.taskId, taskId), eq(taskDays.isRunning, true)))
    .orderBy(desc(taskDays.workDate), desc(taskDays.id))
    .limit(1)
    .get();
}

/** The task's most recent day row, for a read-only response when there is nothing to mutate. */
function getLatestDay(taskId: number): TaskDay | undefined {
  return db
    .select()
    .from(taskDays)
    .where(eq(taskDays.taskId, taskId))
    .orderBy(desc(taskDays.workDate), desc(taskDays.id))
    .limit(1)
    .get();
}

/**
 * The day row a stop/reset acts on: the named date when the caller names one,
 * otherwise the live run (its date is whatever day it was started), otherwise
 * the named date, otherwise nothing to do.
 */
function resolveDay(taskId: number, workDate?: string): TaskDay | undefined {
  const running = getRunningDay(taskId);
  if (workDate) return getDay(taskId, workDate) ?? running;
  return running ?? getDay(taskId, todayISO());
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

/**
 * The day view: every task that has a row on `workDate`, flattened, in that
 * day's order. A task with no row for the date is not on that date's list.
 * `filter` narrows it further, the way the archive's params do.
 */
export function listTasks(
  userId: string,
  workDate: string,
  filter: TaskFilter = {}
): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const conds = [eq(tasks.userId, userId), eq(taskDays.workDate, workDate)];
  if (filter.status !== undefined) conds.push(eq(taskDays.status, filter.status));
  if (filter.done !== undefined) conds.push(eq(taskDays.done, filter.done));
  if (filter.archived !== undefined) conds.push(eq(taskDays.isArchived, filter.archived));
  if (filter.q) conds.push(like(tasks.label, `%${filter.q}%`));
  if (filter.tag) conds.push(like(tasks.tags, `%${filter.tag}%`));
  const rows = db
    .select({ task: tasks, day: taskDays })
    .from(tasks)
    .innerJoin(taskDays, eq(taskDays.taskId, tasks.id))
    .where(and(...conds))
    .orderBy(asc(taskDays.position), asc(taskDays.id))
    .all();
  const dtos = rows.map((row) => toDTO(row.task, row.day));
  // The total is the current (stored + live) elapsed of exactly the rows returned.
  return { tasks: dtos, totalElapsedSeconds: dtos.reduce((sum, t) => sum + t.currentElapsedSeconds, 0) };
}

/**
 * The shared filter for both list shapes. `q`/`tag` select the TASK (label and
 * tags live on the identity); `status`/`done`/`archived` select the DAY, since
 * the lifecycle flags are per day.
 */
export interface TaskFilter {
  q?: string;
  tag?: string;
  status?: string;
  done?: boolean;
  archived?: boolean;
}

/**
 * The archive: every task with all of its days, so a task worked on ten days is
 * one row carrying ten recorded days. `q`/`tag` filter the identity;
 * `status`/`done`/`archived` filter which days appear; a task whose days are all
 * filtered out is dropped.
 */
export function listArchive(
  userId: string,
  filter: TaskFilter = {}
): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const conds = [eq(tasks.userId, userId)];
  if (filter.q) conds.push(like(tasks.label, `%${filter.q}%`));
  if (filter.tag) conds.push(like(tasks.tags, `%${filter.tag}%`));
  const identities = db
    .select()
    .from(tasks)
    .where(conds.length === 1 ? conds[0] : and(...conds))
    .all();
  if (identities.length === 0) return { tasks: [], totalElapsedSeconds: 0 };

  const days = db
    .select()
    .from(taskDays)
    .where(inArray(taskDays.taskId, identities.map((t) => t.id)))
    .orderBy(asc(taskDays.workDate), asc(taskDays.id))
    .all();

  const byTask = new Map<number, TaskDay[]>();
  for (const day of days) {
    if (filter.status !== undefined && day.status !== filter.status) continue;
    if (filter.done !== undefined && day.done !== filter.done) continue;
    if (filter.archived !== undefined && day.isArchived !== filter.archived) continue;
    const list = byTask.get(day.taskId);
    if (list) list.push(day);
    else byTask.set(day.taskId, [day]);
  }

  const dtos: TaskDTO[] = [];
  for (const identity of identities) {
    const own = byTask.get(identity.id);
    if (!own) continue; // no day survived the filter — the task is not in the archive
    const dayDtos = own.map(toDayDTO);
    const dto = toDTO(identity, null, dayDtos);
    // The archive is a roll-up of the task's days, so its own totals are the sum.
    dto.elapsedSeconds = dayDtos.reduce((sum, d) => sum + d.elapsedSeconds, 0);
    dto.currentElapsedSeconds = dayDtos.reduce((sum, d) => sum + d.currentElapsedSeconds, 0);
    dto.totalTime = dayDtos.reduce((sum, d) => sum + d.totalTime, 0);
    dtos.push(dto);
  }
  return {
    tasks: dtos,
    totalElapsedSeconds: dtos.reduce((sum, t) => sum + t.currentElapsedSeconds, 0)
  };
}

export interface TaskCreate {
  label: string;
  description: string | null;
  workDate?: string; // YYYY-MM-DD; default today
  code?: string | null;
  link?: string | null;
  status?: string;
  notes?: string | null;
  tags?: string[] | null;
}

/**
 * Create a task, or return the existing task with the same label. The label is
 * the identity, so a second create on a new day is the same task gaining a day —
 * never a second task. The identity is the description now: there is no
 * "copy yesterday's description" step, because yesterday's row *is* this task.
 */
export async function createTask(userId: string, input: TaskCreate): Promise<TaskDTO> {
  const workDate = input.workDate ?? todayISO();
  const label = input.label.trim();

  let task = findIdentity(db, userId, label);
  let created = false;
  if (!task) {
    // JIRA summary fetch (fire-and-forget, new-identity path only).
    let description = input.description ?? null;
    try {
      const fetched = await jira.onCreate(label, description);
      if (fetched) description = fetched;
    } catch (err) {
      console.error('[taskService] jira.onCreate error:', err instanceof Error ? err.message : err);
    }
    task = insertIdentity(db, userId, { ...input, label, workDate, description });
    created = true;
  }

  // Starting a task on a new day creates that day's row; an existing day is
  // returned unchanged, so re-creating a task never resets its day.
  const existingDay = getDay(task.id, workDate);
  const day = existingDay ?? ensureDayRow(db, userId, task.id, workDate, input.status);
  if (created || !existingDay) publish(userId, { entity: 'task', taskId: task.id, action: 'create' });
  return toDTO(task, day);
}

/** Stop one running day row; returns the run's elapsed delta (seconds) for worklogging. */
function stopDay(row: TaskDay): number {
  const elapsed = currentElapsedSeconds(
    row.elapsedTime,
    row.isRunning,
    row.startTime ? row.startTime.getTime() : null
  );
  db.update(taskDays)
    .set({ isRunning: false, elapsedTime: elapsed, startTime: null, updatedAt: new Date() })
    .where(eq(taskDays.id, row.id))
    .run();
  return elapsed - row.elapsedTime;
}

/**
 * Start a timer on the named day (default today), adding that day to the task
 * when it does not have one yet.
 */
export function startTimer(
  userId: string,
  taskId: number,
  workDate?: string,
  exclusive?: boolean
): TaskDTO | null {
  const task = getOwnedTask(userId, taskId);
  if (!task) return null;

  const date = workDate ?? todayISO();
  const day = ensureDayRow(db, userId, taskId, date);

  const isExclusive = exclusive ?? getTimerMode(userId) === 'focus';
  if (isExclusive) {
    // A running timer is running whatever day it belongs to, so focus mode stops
    // every other running day row of this user — not only the ones on this date.
    const owned = ownedTasksById(userId);
    const running = db
      .select()
      .from(taskDays)
      .where(
        and(
          eq(taskDays.isRunning, true),
          ne(taskDays.id, day.id),
          inArray(taskDays.taskId, [...owned.keys()])
        )
      )
      .all();
    for (const row of running) {
      const delta = stopDay(row);
      // Focus switch: the interrupted run is over, so its day must stop reading
      // In Progress. Local status lands before the JIRA hook, as in the TUI.
      autoStatus(db, userId, row.taskId, row.workDate);
      // Focus switch: log the interrupted run and send the issue back to To Do.
      void jira.onSwitchStop(owned.get(row.taskId)!, delta, row.startTime ? row.startTime.getTime() : null);
    }
  }

  db.update(taskDays)
    .set({ isRunning: true, startTime: new Date(), updatedAt: new Date() })
    .where(eq(taskDays.id, day.id))
    .run();
  // A running task reads In Progress. Local state before the JIRA call, so the
  // list is correct even while a slow or unreachable JIRA is being asked.
  autoStatus(db, userId, taskId, date);
  publish(userId, { entity: 'task', taskId, action: 'start' });
  void jira.onStart(task);
  return toDTO(task, getDay(taskId, date)!);
}

export function stopTimer(userId: string, taskId: number, workDate?: string): TaskDTO | null {
  const task = getOwnedTask(userId, taskId);
  if (!task) return null;

  const day = resolveDay(taskId, workDate);
  // Nothing on that day and nothing running: nothing to stop, so report the
  // task's latest day rather than inventing one.
  if (!day) return toDTO(task, getLatestDay(taskId));

  const startMs = day.startTime ? day.startTime.getTime() : null;
  const delta = stopDay(day);
  // A run ending on an unfinished day reads To Do again — the day must not keep
  // showing In Progress with no timer behind it. A day already checked done
  // keeps Done, because the state after this stop is still done.
  autoStatus(db, userId, taskId, day.workDate);
  publish(userId, { entity: 'task', taskId, action: 'stop' });
  void jira.onStop(task, delta, startMs); // worklog only; keep the issue's status
  return toDTO(task, getDay(taskId, day.workDate)!);
}

export function resetTask(userId: string, taskId: number, workDate?: string): TaskDTO | null {
  const task = getOwnedTask(userId, taskId);
  if (!task) return null;

  const day = resolveDay(taskId, workDate);
  if (!day) return toDTO(task, getLatestDay(taskId));

  db.update(taskDays)
    .set({ isRunning: false, elapsedTime: 0, startTime: null, updatedAt: new Date() })
    .where(eq(taskDays.id, day.id))
    .run();
  // A reset stops the timer, so the day is no longer In Progress.
  autoStatus(db, userId, taskId, day.workDate);
  publish(userId, { entity: 'task', taskId, action: 'reset' });
  return toDTO(task, getDay(taskId, day.workDate)!);
}

/** Zero every one of the user's day rows on the given date (default today). */
export function resetAll(
  userId: string,
  workDate?: string
): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const date = workDate ?? todayISO();
  const owned = ownedTasksById(userId);
  const days = owned.size
    ? db
        .select()
        .from(taskDays)
        .where(and(eq(taskDays.workDate, date), inArray(taskDays.taskId, [...owned.keys()])))
        .all()
    : [];

  if (days.length > 0) {
    db.update(taskDays)
      .set({ isRunning: false, elapsedTime: 0, startTime: null, updatedAt: new Date() })
      .where(inArray(taskDays.id, days.map((d) => d.id)))
      .run();
    // A reset stops every timer, so each day must stop reading In Progress.
    for (const day of days) autoStatus(db, userId, day.taskId, date);
  }
  publish(userId, { entity: 'task', taskId: 0, action: 'reset_all' });
  return listTasks(userId, date);
}

export function listComments(userId: string, taskId: number) {
  if (!getOwnedTask(userId, taskId)) return null;
  return db.select().from(taskComments).where(eq(taskComments.taskId, taskId)).all();
}

export function addComment(userId: string, taskId: number, input: {
  subject?: string | null; summary?: string | null; branch?: string | null; pr?: string | null;
}) {
  if (!getOwnedTask(userId, taskId)) return null;
  const row = db.insert(taskComments).values({ taskId, ...input }).returning().get();
  publish(userId, { entity: 'comment', taskId });
  return row;
}

export function listIntegrations(userId: string, taskId: number) {
  if (!getOwnedTask(userId, taskId)) return null;
  return db.select().from(taskIntegrations).where(eq(taskIntegrations.taskId, taskId)).all();
}

export function upsertIntegration(userId: string, taskId: number, group: string, field: string, value: string | null) {
  if (!getOwnedTask(userId, taskId)) return null;
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

/** Delete the task identity; its days, comments and integrations cascade. */
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

/**
 * Split an update by where the field lives: label/description/code/link/notes/
 * tags are the identity (shared by every day), everything else is the day row
 * for `workDate` (default today).
 */
export function updateTask(
  userId: string,
  taskId: number,
  changes: TaskUpdate,
  workDate?: string
): TaskDTO | null {
  const task = getOwnedTask(userId, taskId);
  if (!task) return null;

  const date = workDate ?? todayISO();
  const explicitDate = workDate !== undefined;

  // Identity fields — written on the task row itself.
  const identity: Partial<Task> = {};
  if (changes.label !== undefined) identity.label = changes.label;
  if (changes.description !== undefined) identity.description = changes.description;
  if (changes.code !== undefined) identity.code = changes.code;
  if (changes.link !== undefined) identity.link = changes.link;
  if (changes.notes !== undefined) identity.notes = changes.notes;
  if (changes.tags !== undefined) identity.tags = changes.tags;

  const dayChanges =
    changes.elapsedSeconds !== undefined ||
    changes.done !== undefined ||
    changes.status !== undefined ||
    changes.isCompleted !== undefined ||
    changes.isCancelled !== undefined ||
    changes.isDeleted !== undefined ||
    changes.isArchived !== undefined ||
    changes.isPinned !== undefined ||
    changes.isImportant !== undefined;

  // The day the edit lands on: the named date, else the live run (editing the
  // running day is what the user means when they do not name one). Editing a day
  // field on a day the task does not have yet creates it — that is the day the
  // edit is about.
  let day = getDay(taskId, date) ?? (explicitDate ? undefined : getRunningDay(taskId));
  if (!day && dayChanges) day = ensureDayRow(db, userId, taskId, date);

  const daySet: Partial<TaskDay> = {};
  if (changes.status !== undefined) daySet.status = changes.status;
  if (changes.isCompleted !== undefined) daySet.isCompleted = changes.isCompleted;
  if (changes.isCancelled !== undefined) daySet.isCancelled = changes.isCancelled;
  if (changes.isDeleted !== undefined) daySet.isDeleted = changes.isDeleted;
  if (changes.isArchived !== undefined) daySet.isArchived = changes.isArchived;
  if (changes.isPinned !== undefined) daySet.isPinned = changes.isPinned;
  if (changes.isImportant !== undefined) daySet.isImportant = changes.isImportant;
  if (changes.elapsedSeconds !== undefined) {
    daySet.elapsedTime = changes.elapsedSeconds;
    // If running, rebase the current run so live time continues from the new value.
    if (day?.isRunning) daySet.startTime = new Date();
  }

  // Marking a day done (false→true): stop any live run and record its delta so the
  // JIRA hook can worklog it, then move the issue to Cek di Local.
  let doneDelta = 0;
  let doneStartMs: number | null = null;
  const becameDone = changes.done !== undefined && changes.done !== day?.done;
  if (changes.done !== undefined) daySet.done = changes.done;
  if (becameDone && changes.done && day?.isRunning) {
    doneStartMs = day.startTime ? day.startTime.getTime() : null;
    const elapsed = currentElapsedSeconds(day.elapsedTime, day.isRunning, doneStartMs);
    doneDelta = elapsed - day.elapsedTime;
    daySet.isRunning = false;
    daySet.elapsedTime = elapsed;
    daySet.startTime = null;
  }

  const now = new Date();
  if (Object.keys(identity).length > 0) {
    db.update(tasks)
      .set({ ...identity, updatedAt: now })
      .where(and(eq(tasks.id, taskId), eq(tasks.userId, userId)))
      .run();
  }
  if (day && Object.keys(daySet).length > 0) {
    db.update(taskDays)
      .set({ ...daySet, updatedAt: now })
      .where(eq(taskDays.id, day.id))
      .run();
  }
  // Toggling done is a layout change, so it owns the stored status: done → Done,
  // un-done → To Do. An explicit `status` in the same request wins, matching the
  // TUI edit form, which writes a chosen status and does not auto-correct it.
  if (becameDone && changes.status === undefined && day) {
    autoStatus(db, userId, taskId, day.workDate);
  }
  publish(userId, { entity: 'task', taskId, action: 'update' });
  if (becameDone && changes.done) void jira.onDone(task, doneDelta, doneStartMs);
  return toDTO(getOwnedTask(userId, taskId)!, day ? getDay(taskId, day.workDate) : null);
}

export type ReportFormat = 'markdown' | 'csv';

/** Build a Daily Report over the reported day's current elapsed times. */
export function buildReport(
  userId: string,
  format: ReportFormat,
  dateISO: string
): { contentType: string; body: string } {
  const { tasks: dtos } = listTasks(userId, dateISO);
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

/** Set the named day's positions to match the given id order (only the user's own tasks). */
export function reorderTasks(
  userId: string,
  orderedIds: number[],
  workDate?: string
): { tasks: TaskDTO[]; totalElapsedSeconds: number } {
  const date = workDate ?? todayISO();
  const days = db
    .select({ id: taskDays.id, taskId: taskDays.taskId })
    .from(taskDays)
    .innerJoin(tasks, eq(tasks.id, taskDays.taskId))
    .where(and(eq(tasks.userId, userId), eq(taskDays.workDate, date)))
    .all();
  const dayByTask = new Map(days.map((d) => [d.taskId, d.id]));

  db.transaction((tx) => {
    let pos = 0;
    for (const id of orderedIds) {
      const dayId = dayByTask.get(id);
      if (dayId === undefined) continue;
      tx.update(taskDays)
        .set({ position: pos, updatedAt: new Date() })
        .where(eq(taskDays.id, dayId))
        .run();
      pos++;
    }
  });
  publish(userId, { entity: 'task', taskId: 0, action: 'reorder' });
  return listTasks(userId, date);
}
