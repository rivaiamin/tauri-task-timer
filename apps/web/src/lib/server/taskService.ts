// Domain layer for task/timer operations, scoped by user. Used by the REST API
// (and later the MCP server). The web UI keeps its own optimistic copy of this
// logic for snappiness; both share the pure math in `shared` so they stay aligned.
import { supabaseAdmin } from '$lib/supabaseServer';
import { currentElapsedSeconds } from 'shared';
import type { DatabaseTask } from 'shared';

export type TimerMode = 'focus' | 'parallel';

export interface TaskDTO {
  id: number;
  label: string;
  description: string | null;
  position: number;
  is_running: boolean;
  start_time: string | null;
  elapsed_time: number;
  current_elapsed_seconds: number;
}

function startMs(startTime: string | null): number | null {
  return startTime ? new Date(startTime).getTime() : null;
}

function toDTO(t: DatabaseTask): TaskDTO {
  return {
    id: t.id,
    label: t.label,
    description: t.description ?? null,
    position: t.position,
    is_running: t.is_running,
    start_time: t.start_time,
    elapsed_time: t.elapsed_time,
    current_elapsed_seconds: currentElapsedSeconds(t.elapsed_time, t.is_running, startMs(t.start_time))
  };
}

export async function getTimerMode(userId: string): Promise<TimerMode> {
  const { data } = await supabaseAdmin
    .from('user_settings')
    .select('timer_mode')
    .eq('user_id', userId)
    .maybeSingle();
  return (data?.timer_mode as TimerMode) ?? 'focus';
}

export async function setTimerMode(userId: string, mode: TimerMode): Promise<{ timer_mode: TimerMode }> {
  const { error } = await supabaseAdmin
    .from('user_settings')
    .upsert(
      { user_id: userId, timer_mode: mode, updated_at: new Date().toISOString() },
      { onConflict: 'user_id' }
    );
  if (error) throw new Error(error.message);
  return { timer_mode: mode };
}

export async function listTasks(userId: string): Promise<{ tasks: TaskDTO[]; total_elapsed_seconds: number }> {
  const { data, error } = await supabaseAdmin
    .from('tasks')
    .select('*')
    .eq('user_id', userId)
    .order('position', { ascending: true });
  if (error) throw new Error(error.message);

  const tasks = (data ?? []).map(toDTO);
  const total = tasks.reduce((sum, t) => sum + t.current_elapsed_seconds, 0);
  return { tasks, total_elapsed_seconds: total };
}

export async function createTask(userId: string, label: string, description: string | null): Promise<TaskDTO> {
  const { count } = await supabaseAdmin
    .from('tasks')
    .select('*', { count: 'exact', head: true })
    .eq('user_id', userId);

  const { data, error } = await supabaseAdmin
    .from('tasks')
    .insert({
      user_id: userId,
      label,
      description: description ?? null,
      elapsed_time: 0,
      position: count ?? 0,
      is_running: false
    })
    .select()
    .single();
  if (error) throw new Error(error.message);
  return toDTO(data);
}

async function getOwnedTask(userId: string, taskId: number): Promise<DatabaseTask | null> {
  const { data, error } = await supabaseAdmin
    .from('tasks')
    .select('*')
    .eq('id', taskId)
    .eq('user_id', userId)
    .maybeSingle();
  if (error) throw new Error(error.message);
  return (data as DatabaseTask | null) ?? null;
}

async function stopRow(userId: string, task: DatabaseTask): Promise<void> {
  const elapsed = currentElapsedSeconds(task.elapsed_time, task.is_running, startMs(task.start_time));
  const { error } = await supabaseAdmin
    .from('tasks')
    .update({ is_running: false, elapsed_time: elapsed, start_time: null })
    .eq('id', task.id)
    .eq('user_id', userId);
  if (error) throw new Error(error.message);
}

/**
 * Start a task's timer. `exclusive` controls focus vs parallel behavior; when
 * omitted it defaults to the user's stored timer mode (focus => exclusive).
 * Returns null if the task doesn't exist / isn't owned by the user.
 */
export async function startTimer(userId: string, taskId: number, exclusive?: boolean): Promise<TaskDTO | null> {
  const task = await getOwnedTask(userId, taskId);
  if (!task) return null;

  const isExclusive = exclusive ?? ((await getTimerMode(userId)) === 'focus');
  if (isExclusive) {
    const { data: running, error } = await supabaseAdmin
      .from('tasks')
      .select('*')
      .eq('user_id', userId)
      .eq('is_running', true)
      .neq('id', taskId);
    if (error) throw new Error(error.message);
    for (const row of (running ?? []) as DatabaseTask[]) {
      await stopRow(userId, row);
    }
  }

  const { data, error } = await supabaseAdmin
    .from('tasks')
    .update({ is_running: true, start_time: new Date().toISOString() })
    .eq('id', taskId)
    .eq('user_id', userId)
    .select()
    .single();
  if (error) throw new Error(error.message);
  return toDTO(data);
}

/** Stop a task's timer, accumulating elapsed time. Null if not found. */
export async function stopTimer(userId: string, taskId: number): Promise<TaskDTO | null> {
  const task = await getOwnedTask(userId, taskId);
  if (!task) return null;

  const elapsed = currentElapsedSeconds(task.elapsed_time, task.is_running, startMs(task.start_time));
  const { data, error } = await supabaseAdmin
    .from('tasks')
    .update({ is_running: false, elapsed_time: elapsed, start_time: null })
    .eq('id', taskId)
    .eq('user_id', userId)
    .select()
    .single();
  if (error) throw new Error(error.message);
  return toDTO(data);
}
