<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { formatTime, currentElapsedSeconds } from 'shared';
  import { todayISO } from '$lib/dates';
  import type { PageData } from './$types';

  export let data: PageData;

  interface TaskDTO {
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
    startTime: number | null;
    elapsedSeconds: number;
    currentElapsedSeconds: number;
    workDate: string;
  }

  let tasks: TaskDTO[] = data.tasks as TaskDTO[];
  let now = Date.now();

  // Reactive sync when SvelteKit re-runs load
  $: tasks = data.tasks;

  let tickInterval: ReturnType<typeof setInterval> | null = null;
  let sse: EventSource | null = null;
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;

  // Filter state (synced to URL)
  let filterQ = '';
  let filterTag = '';
  let filterStatus = '';

  function elapsed(task: TaskDTO, atNow: number): number {
    return currentElapsedSeconds(task.elapsedSeconds, task.isRunning, task.startTime, atNow);
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
      const params = new URLSearchParams();
      if (filterQ) params.set('q', filterQ);
      if (filterTag) params.set('tag', filterTag);
      if (filterStatus) params.set('status', filterStatus);
      const qs = params.toString();
      const d = await api(`/tasks?archived=true${qs ? '&' + qs : ''}`);
      tasks = d.tasks;
    } catch (e) {
      console.error('refresh failed', e);
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(refresh, 120);
  }

  function applyFilters() {
    const params = new URLSearchParams();
    if (filterQ) params.set('q', filterQ);
    if (filterTag) params.set('tag', filterTag);
    if (filterStatus) params.set('status', filterStatus);
    const qs = params.toString();
    goto(`/dashboard/archive${qs ? '?' + qs : ''}`, { replaceState: true, keepFocus: true });
    scheduleRefresh();
  }

  function handleFilterKey(e: KeyboardEvent) {
    if (e.key === 'Enter') applyFilters();
  }

  async function continueToday(task: TaskDTO) {
    try {
      await api('/tasks', {
        method: 'POST',
        body: { label: task.label, description: task.description, workDate: todayISO() }
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
      <p class="text-gray-500 dark:text-gray-400 mt-1">Unfinished tasks across all days.</p>
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
      <button
        on:click={applyFilters}
        class="px-4 py-2.5 text-sm font-semibold bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors shadow-sm"
        type="button"
      >
        Filter
      </button>
    </div>

    <!-- Task list -->
    <div class="space-y-3">
      {#if tasks.length === 0}
        <p class="text-gray-500 dark:text-gray-400 text-center">No unfinished tasks matching filters.</p>
      {:else}
        {#each tasks as task (task.id)}
          <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 dark:bg-gray-800/70 p-4 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 transition duration-200">
            <div class="flex-1 mb-3 sm:mb-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-xs font-mono text-gray-400 dark:text-gray-500">{task.workDate}</span>
                {#if task.code}
                  <span class="inline-block px-1.5 py-0.5 text-xs font-mono bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded">{task.code}</span>
                {/if}
                {#if task.status && task.status !== 'todo'}
                  <span class="inline-block px-1.5 py-0.5 text-xs font-semibold rounded
                    {task.status === 'done' ? 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300' :
                     task.status === 'blocked' ? 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300' :
                     task.status === 'in_progress' ? 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300' :
                     'bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400'}">{task.status}</span>
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
              <span class="text-2xl font-mono text-gray-700 dark:text-gray-300 block mt-1">{formatTime(elapsed(task, now))}</span>
            </div>
            <button
              on:click={() => continueToday(task)}
              class="shrink-0 px-4 py-2 text-sm font-semibold bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors shadow-sm"
              type="button"
            >
              Continue today
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
