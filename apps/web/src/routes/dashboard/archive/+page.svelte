<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { formatTime, currentElapsedSeconds } from 'shared';
  import { todayISO } from '$lib/dates';
  import type { PageData } from './$types';

  import { STATUS_LABELS, statusChipClass } from '$lib/status';

  export let data: PageData;

  /** The fields this page reads off the archive's per-day DTO. */
  interface TaskDayDTO {
    id: number;
    workDate: string;
    status: string;
    elapsedSeconds: number;
    isRunning: boolean;
    startTime: number | null;
    done: boolean;
  }

  /** The fields this page reads off the archive's per-task DTO. */
  interface TaskDTO {
    id: number;
    label: string;
    description: string | null;
    code: string | null;
    tags: string[] | null;
    days?: TaskDayDTO[];
  }

  let tasks: TaskDTO[] = data.tasks as TaskDTO[];
  let totalElapsed = data.totalElapsedSeconds as number;
  let now = Date.now();

  // Reactive sync when SvelteKit re-runs load
  $: tasks = data.tasks;
  $: totalElapsed = data.totalElapsedSeconds;

  let tickInterval: ReturnType<typeof setInterval> | null = null;
  let sse: EventSource | null = null;
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;

  // Filter state (synced to URL)
  let filterQ = '';
  let filterTag = '';
  let filterStatus = '';
  let filterDone = '';

  function dayElapsed(day: TaskDayDTO, atNow: number): number {
    return currentElapsedSeconds(day.elapsedSeconds, day.isRunning, day.startTime, atNow);
  }

  /** A task's total across every day it was worked. */
  function taskTotal(task: TaskDTO, atNow: number): number {
    return (task.days ?? []).reduce((sum, d) => sum + dayElapsed(d, atNow), 0);
  }

  function taskIsRunning(task: TaskDTO): boolean {
    return (task.days ?? []).some((d) => d.isRunning);
  }

  function filterParams(): URLSearchParams {
    const params = new URLSearchParams();
    if (filterQ) params.set('q', filterQ);
    if (filterTag) params.set('tag', filterTag);
    if (filterStatus) params.set('status', filterStatus);
    if (filterDone) params.set('done', filterDone);
    return params;
  }

  onMount(() => {
    tickInterval = setInterval(() => (now = Date.now()), 1000);

    sse = new EventSource('/api/stream');
    sse.onmessage = (e) => {
      try {
        const payload = JSON.parse(e.data);
        if (payload.type === 'change') scheduleRefresh();
      } catch {
        // ignore
      }
    };

    // Restore filter state from URL
    const params = new URLSearchParams(window.location.search);
    filterQ = params.get('q') || '';
    filterTag = params.get('tag') || '';
    filterStatus = params.get('status') || '';
    filterDone = params.get('done') || '';
  });

  onDestroy(() => {
    if (tickInterval) clearInterval(tickInterval);
    if (refreshTimer) clearTimeout(refreshTimer);
    sse?.close();
  });

  async function api(path: string, opts: { method?: string; body?: unknown } = {}) {
    const res = await fetch(`/api${path}`, {
      method: opts.method ?? 'GET',
      headers: opts.body !== undefined ? { 'content-type': 'application/json' } : undefined,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined
    });
    if (!res.ok) {
      let message = res.statusText;
      try {
        const e = await res.json();
        message = e.message ?? message;
      } catch {
        // non-JSON error
      }
      throw new Error(message);
    }
    return res.status === 204 ? null : await res.json();
  }

  async function refresh() {
    try {
      const qs = filterParams().toString();
      const d = await api(`/tasks${qs ? '?' + qs : ''}`);
      tasks = d.tasks;
      totalElapsed = d.totalElapsedSeconds;
    } catch (e) {
      console.error('refresh failed', e);
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(refresh, 120);
  }

  function applyFilters() {
    const qs = filterParams().toString();
    goto(`/dashboard/archive${qs ? '?' + qs : ''}`, { replaceState: true, keepFocus: true });
    scheduleRefresh();
  }

  function handleFilterKey(e: KeyboardEvent) {
    if (e.key === 'Enter') applyFilters();
  }

  async function continueToday(task: TaskDTO) {
    try {
      const description = task.description?.trim();
      await api('/tasks', {
        method: 'POST',
        // The identity IS the description now; a null description must be omitted
        // (the endpoint's `description` is an optional string, not nullable).
        body: { label: task.label, ...(description ? { description } : {}), workDate: todayISO() }
      });
      goto('/dashboard');
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to continue task');
    }
  }
</script>

<div class="min-h-screen p-4 sm:p-8 bg-gray-50 text-gray-900 dark:bg-gray-950 dark:text-gray-100">
  <div class="max-w-2xl mx-auto bg-white dark:bg-gray-900 rounded-xl shadow-lg p-6 sm:p-8 ring-1 ring-black/5 dark:ring-white/10">
    <header class="mb-6">
      <a href="/dashboard" class="text-sm font-semibold text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300">&larr; Back to dashboard</a>
      <h1 class="text-3xl font-bold text-gray-900 dark:text-gray-100 mt-2">Archive</h1>
      <p class="text-gray-500 dark:text-gray-400 mt-1">Every task and each day it was worked.</p>
      {#if tasks.length > 0}
        <p class="text-sm text-gray-400 dark:text-gray-500 mt-1">
          {tasks.length} {tasks.length === 1 ? 'task' : 'tasks'} &middot; {formatTime(totalElapsed)} tracked
        </p>
      {/if}
    </header>

    <!-- Filter bar -->
    <div class="flex flex-col sm:flex-row gap-2 mb-6">
      <input
        type="text"
        bind:value={filterQ}
        on:keydown={handleFilterKey}
        placeholder="Filter by label..."
        class="flex-1 p-2.5 text-sm border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
      />
      <input
        type="text"
        bind:value={filterTag}
        on:keydown={handleFilterKey}
        placeholder="Filter by tag..."
        class="w-full sm:w-40 p-2.5 text-sm border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
      />
      <select
        bind:value={filterStatus}
        on:change={applyFilters}
        class="w-full sm:w-36 p-2.5 text-sm border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
      >
        <option value="">Any status</option>
        <option value="todo">todo</option>
        <option value="in_progress">in_progress</option>
        <option value="blocked">blocked</option>
      </select>
      <select
        bind:value={filterDone}
        on:change={applyFilters}
        class="w-full sm:w-32 p-2.5 text-sm border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
        title="Filter which days are shown"
      >
        <option value="">Any day</option>
        <option value="false">Not done</option>
        <option value="true">Done</option>
      </select>
      <button
        on:click={applyFilters}
        class="px-4 py-2.5 text-sm font-semibold bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors shadow-sm"
        type="button"
      >
        Filter
      </button>
    </div>

    <!-- Task list: one entry per task, each with its per-day breakdown -->
    <div class="space-y-3">
      {#if tasks.length === 0}
        <p class="text-gray-500 dark:text-gray-400 text-center">No tasks matching filters.</p>
      {:else}
        {#each tasks as task (task.id)}
          <div class="bg-gray-50 dark:bg-gray-800/70 p-4 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 transition duration-200 {taskIsRunning(task) ? 'ring-2 ring-green-400 dark:ring-green-500' : ''}">
            <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between">
              <div class="flex-1 mb-3 sm:mb-0">
                <div class="flex items-center gap-2 flex-wrap">
                  {#if task.code}
                    <span class="inline-block px-1.5 py-0.5 text-xs font-mono bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded">{task.code}</span>
                  {/if}
                  {#if task.tags}
                    {#each task.tags as tag}
                      <span class="inline-block px-1.5 py-0.5 text-xs bg-indigo-50 dark:bg-indigo-950/40 text-indigo-600 dark:text-indigo-400 rounded">{tag}</span>
                    {/each}
                  {/if}
                </div>
                <span class="text-lg font-medium text-gray-900 dark:text-gray-100 break-words mt-1 block">{task.label}</span>
                {#if task.description}
                  <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5 line-clamp-1">{task.description}</p>
                {/if}
              </div>
              <button
                on:click={() => continueToday(task)}
                class="shrink-0 px-4 py-2 text-sm font-semibold bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors shadow-sm"
                type="button"
              >
                Continue today
              </button>
            </div>

            <!-- Per-day breakdown: every day this task was worked, with its own time -->
            {#if task.days && task.days.length}
              <div class="mt-3 pt-3 border-t border-gray-200 dark:border-gray-700">
                <div class="flex items-center justify-between mb-2">
                  <span class="text-xs font-semibold uppercase tracking-wide text-gray-400 dark:text-gray-500">
                    {task.days.length} {task.days.length === 1 ? 'day' : 'days'}
                  </span>
                  <span class="text-lg font-mono font-semibold text-gray-700 dark:text-gray-200">{formatTime(taskTotal(task, now))}</span>
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each task.days as day (day.id)}
                    <span
                      class="inline-flex items-center gap-1.5 px-2 py-1 text-xs rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 {day.isRunning ? 'ring-1 ring-green-400 dark:ring-green-500' : ''} {day.done ? 'opacity-70' : ''}"
                      title={day.isRunning ? 'Running' : undefined}
                    >
                      <span class="font-mono text-gray-500 dark:text-gray-400">{day.workDate}</span>
                      <span class="font-mono font-semibold text-gray-800 dark:text-gray-200 {day.done ? 'line-through' : ''}">{formatTime(dayElapsed(day, now))}</span>
                      {#if day.done}
                        <span class="text-green-600 dark:text-green-400" title="Done">&check;</span>
                      {/if}
                      {#if day.status && day.status !== 'todo'}
                        <span class="px-1 py-0.5 text-[10px] font-semibold rounded {statusChipClass(day.status)}">{STATUS_LABELS[day.status] ?? day.status}</span>
                      {/if}
                    </span>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
