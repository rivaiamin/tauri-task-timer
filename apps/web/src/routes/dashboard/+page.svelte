<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { formatTime, currentElapsedSeconds, buildCsvReport, buildMarkdownReport } from 'shared';
  import type { PageData } from './$types';

  export let data: PageData;

  interface TaskDTO {
    id: number;
    label: string;
    description: string | null;
    position: number;
    isRunning: boolean;
    done: boolean;
    startTime: number | null;
    elapsedSeconds: number;
    currentElapsedSeconds: number;
  }

  let tasks: TaskDTO[] = data.tasks as TaskDTO[];
  let timerMode: 'focus' | 'parallel' = data.timerMode as 'focus' | 'parallel';
  let workDate: string = data.workDate as string;

  // Re-sync when SvelteKit re-runs the load function (e.g. after goto).
  $: tasks = data.tasks;
  $: workDate = data.workDate;
  $: timerMode = data.timerMode;

  let taskInput = '';
  let now = Date.now();
  let hasPlayed8HourSound = false;
  let pulse8Hour = false;

  let tickInterval: ReturnType<typeof setInterval> | null = null;
  let sse: EventSource | null = null;
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;

  // Inline "Edit Mode"
  let editMode = false;
  let editBuffers: Record<string, { label: string; description: string; time: string }> = {};

  // Drag-and-drop
  let draggingId: number | null = null;
  let dragOverId: number | null = null;
  let dragInsertAfter = false;

  // Edit modal
  let showEditModal = false;
  let editingTask: TaskDTO | null = null;
  let editTitle = '';
  let editDescription = '';
  let editTime = '';

  function elapsed(task: TaskDTO, atNow: number): number {
    return currentElapsedSeconds(task.elapsedSeconds, task.isRunning, task.startTime, atNow);
  }

  function play8HourSound() {
    try {
      const ctx = new (window.AudioContext || (window as any).webkitAudioContext)();
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.frequency.value = 800;
      osc.type = 'sine';
      gain.gain.setValueAtTime(0.3, ctx.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.5);
      osc.start(ctx.currentTime);
      osc.stop(ctx.currentTime + 0.5);
    } catch {
      // audio not available
    }
  }

  $: totalSeconds = tasks.reduce((sum, t) => sum + elapsed(t, now), 0);

  $: {
    if (totalSeconds >= 28800 && !hasPlayed8HourSound) {
      play8HourSound();
      hasPlayed8HourSound = true;
      pulse8Hour = true;
      setTimeout(() => (pulse8Hour = false), 2000);
    } else if (totalSeconds < 28800) {
      hasPlayed8HourSound = false;
    }
  }

  // Ensure every rendered task has an edit buffer while in edit mode.
  $: if (editMode) {
    for (const t of tasks) {
      if (!editBuffers[t.id]) {
        editBuffers[t.id] = {
          label: t.label,
          description: t.description || '',
          time: formatTime(elapsed(t, now))
        };
      }
    }
  }

  onMount(() => {
    tickInterval = setInterval(() => (now = Date.now()), 1000);

    sse = new EventSource('/api/stream');
    sse.onmessage = (e) => {
      try {
        const payload = JSON.parse(e.data);
        if (payload.type === 'change') scheduleRefresh();
      } catch {
        // ignore malformed event
      }
    };
    sse.onerror = () => {
      // Browser auto-reconnects EventSource; nothing to do.
    };

    document.addEventListener('visibilitychange', onVisible);
  });

  onDestroy(() => {
    if (tickInterval) clearInterval(tickInterval);
    if (refreshTimer) clearTimeout(refreshTimer);
    sse?.close();
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', onVisible);
  });

  function onVisible() {
    if (!document.hidden) {
      now = Date.now();
      scheduleRefresh();
    }
  }

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
      const d = await api(`/tasks?date=${workDate}`);
      tasks = d.tasks;
    } catch (e) {
      console.error('refresh failed', e);
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(refresh, 120);
  }

  function shiftDate(iso: string, days: number): string {
    const d = new Date(iso + 'T12:00:00');
    d.setDate(d.getDate() + days);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  function navigateDate(iso: string) {
    workDate = iso;
    goto(`/dashboard?date=${iso}`, { replaceState: true, keepFocus: true });
    scheduleRefresh();
  }

  function prevDay() {
    navigateDate(shiftDate(workDate, -1));
  }

  function nextDay() {
    navigateDate(shiftDate(workDate, 1));
  }

  function onDateInput(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    if (/^\d{4}-\d{2}-\d{2}$/.test(v)) navigateDate(v);
  }

  function goToday() {
    const now = new Date();
    const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    if (workDate !== today) navigateDate(today);
  }

  $: isToday = workDate === (() => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  })();

  function fail(e: unknown) {
    alert(e instanceof Error ? e.message : 'Something went wrong');
  }

  async function addTask(event: Event) {
    event.preventDefault();
    const label = taskInput.trim();
    if (!label) return;
    taskInput = '';
    try {
      await api('/tasks', { method: 'POST', body: { label, workDate } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function toggleTimer(task: TaskDTO) {
    if (editMode) return;
    try {
      if (task.isRunning) {
        await api(`/tasks/${task.id}/stop`, { method: 'POST' });
      } else {
        await api(`/tasks/${task.id}/start`, { method: 'POST', body: { exclusive: timerMode === 'focus' } });
      }
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function toggleDone(task: TaskDTO, done: boolean) {
    try {
      await api(`/tasks/${task.id}`, { method: 'PATCH', body: { done } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function resetTimer(id: number) {
    try {
      await api(`/tasks/${id}/reset`, { method: 'POST' });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function deleteTask(id: number, skipConfirm = false) {
    if (!skipConfirm && !confirm('Are you sure you want to delete this task?')) return;
    try {
      await api(`/tasks/${id}`, { method: 'DELETE' });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function resetAllTimers() {
    if (tasks.length === 0) {
      alert('No tasks to reset.');
      return;
    }
    if (!confirm('Are you sure you want to reset all timers to 00:00:00?')) return;
    try {
      await api('/tasks/reset-all', { method: 'POST', body: { workDate } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function persistOrder(orderedIds: number[]) {
    try {
      await api('/tasks/reorder', { method: 'POST', body: { ids: orderedIds } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
      refresh();
    }
  }

  function moveTaskUp(id: number) {
    const i = tasks.findIndex((t) => t.id === id);
    if (i <= 0) return;
    const a = [...tasks];
    [a[i - 1], a[i]] = [a[i], a[i - 1]];
    tasks = a;
    persistOrder(a.map((t) => t.id));
  }

  function moveTaskDown(id: number) {
    const i = tasks.findIndex((t) => t.id === id);
    if (i < 0 || i >= tasks.length - 1) return;
    const a = [...tasks];
    [a[i], a[i + 1]] = [a[i + 1], a[i]];
    tasks = a;
    persistOrder(a.map((t) => t.id));
  }

  // ---- Drag-and-drop ----
  function handleDragStart(event: DragEvent, id: number) {
    if (editMode) return;
    if (event.target instanceof Element && event.target.closest('button')) {
      event.preventDefault();
      return;
    }
    draggingId = id;
    dragOverId = null;
    dragInsertAfter = false;
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = 'move';
      event.dataTransfer.setData('text/plain', String(id));
    }
  }

  function handleDragOver(event: DragEvent, id: number) {
    if (editMode || draggingId === null || id === draggingId) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    dragInsertAfter = event.clientY > rect.top + rect.height / 2;
    dragOverId = id;
  }

  function handleDrop(event: DragEvent) {
    if (editMode || draggingId === null) return;
    event.preventDefault();
    const sourceId = draggingId;
    const targetId = dragOverId;
    const after = dragInsertAfter;
    draggingId = null;
    dragOverId = null;
    dragInsertAfter = false;
    if (targetId === null || sourceId === targetId) return;

    const a = [...tasks];
    const si = a.findIndex((t) => t.id === sourceId);
    if (si < 0) return;
    const [moved] = a.splice(si, 1);
    const ti = a.findIndex((t) => t.id === targetId);
    if (ti < 0) return;
    a.splice(after ? ti + 1 : ti, 0, moved);
    tasks = a;
    persistOrder(a.map((t) => t.id));
  }

  function handleDragEnd() {
    draggingId = null;
    dragOverId = null;
    dragInsertAfter = false;
  }

  function parseTimeInput(value: string): number | null {
    const input = value.trim();
    if (!input) return null;
    if (/^\d+(:\d+){0,2}$/.test(input)) {
      const parts = input.split(':').map(Number);
      if (parts.some((n) => isNaN(n) || n < 0)) return null;
      let hours = 0,
        minutes = 0,
        seconds = 0;
      if (parts.length === 3) [hours, minutes, seconds] = parts;
      else if (parts.length === 2) [minutes, seconds] = parts;
      else [seconds] = parts;
      return hours * 3600 + minutes * 60 + seconds;
    }
    const asNumber = Number(input.replace(',', '.'));
    if (!isFinite(asNumber) || asNumber < 0) return null;
    return Math.round(asNumber * 60);
  }

  // ---- Timer mode ----
  async function toggleTimerMode() {
    const next = timerMode === 'focus' ? 'parallel' : 'focus';
    try {
      await api('/settings/timer-mode', { method: 'PUT', body: { timer_mode: next } });
      timerMode = next;
    } catch (e) {
      fail(e);
    }
  }

  // ---- Inline edit ----
  function toggleEditMode() {
    editMode = !editMode;
    editBuffers = {};
    if (editMode) {
      for (const t of tasks) {
        editBuffers[t.id] = { label: t.label, description: t.description || '', time: formatTime(elapsed(t, now)) };
      }
    }
  }

  async function saveInlineTitle(task: TaskDTO) {
    const buf = editBuffers[task.id];
    if (!buf) return;
    const v = buf.label.trim();
    if (!v) {
      buf.label = task.label;
      editBuffers = editBuffers;
      return;
    }
    if (v === task.label) return;
    try {
      await api(`/tasks/${task.id}`, { method: 'PATCH', body: { label: v } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function saveInlineDescription(task: TaskDTO) {
    const buf = editBuffers[task.id];
    if (!buf) return;
    const v = buf.description.trim();
    if (v === (task.description || '')) return;
    try {
      await api(`/tasks/${task.id}`, { method: 'PATCH', body: { description: v || null } });
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  async function saveInlineTime(task: TaskDTO) {
    const buf = editBuffers[task.id];
    if (!buf) return;
    const secs = parseTimeInput(buf.time);
    if (secs === null || secs < 0) {
      buf.time = formatTime(elapsed(task, now));
      editBuffers = editBuffers;
      return;
    }
    try {
      await api(`/tasks/${task.id}`, { method: 'PATCH', body: { elapsed_seconds: secs } });
      buf.time = formatTime(secs);
      editBuffers = editBuffers;
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  function blurOnEnter(event: KeyboardEvent) {
    if (event.key === 'Enter') (event.currentTarget as HTMLInputElement).blur();
  }

  // ---- Edit modal ----
  function openEditModal(task: TaskDTO) {
    editingTask = task;
    editTitle = task.label;
    editDescription = task.description || '';
    editTime = formatTime(elapsed(task, now));
    showEditModal = true;
  }

  function closeEditModal() {
    showEditModal = false;
    editingTask = null;
    editTitle = '';
    editDescription = '';
    editTime = '';
  }

  async function saveEditTask() {
    if (!editingTask) return;
    const newTitle = editTitle.trim();
    if (!newTitle) {
      alert('Task title cannot be empty');
      return;
    }
    const secs = parseTimeInput(editTime);
    if (secs === null) {
      alert('Invalid time format. Use HH:MM:SS or minutes (e.g. 90)');
      return;
    }
    try {
      await api(`/tasks/${editingTask.id}`, {
        method: 'PATCH',
        body: { label: newTitle, description: editDescription.trim() || null, elapsed_seconds: secs }
      });
      closeEditModal();
      scheduleRefresh();
    } catch (e) {
      fail(e);
    }
  }

  // ---- Exports (client-side, over the in-memory list) ----
  function reportTasks() {
    return tasks.map((t) => ({ label: t.label, description: t.description, elapsedSeconds: elapsed(t, now) }));
  }

  function tasksToCsv(): string {
    return buildCsvReport(reportTasks());
  }

  function exportTasksAsCsv() {
    if (!tasks.length) {
      alert('No tasks to export yet.');
      return;
    }
    const blob = new Blob([tasksToCsv()], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `tasks-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }

  function tasksToMarkdown(): string {
    return buildMarkdownReport(reportTasks(), new Date().toISOString().slice(0, 10));
  }

  async function exportTasksAsMarkdown() {
    const withTime = tasks.filter((t) => elapsed(t, now) > 0);
    if (!withTime.length) {
      alert(tasks.length ? 'No tasks with recorded time to export.' : 'No tasks to export yet.');
      return;
    }
    const content = tasksToMarkdown();
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(content);
      } else {
        const ta = document.createElement('textarea');
        ta.value = content;
        ta.style.position = 'fixed';
        ta.style.left = '-999999px';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
      }
      alert('Daily report copied to clipboard!');
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="min-h-screen p-4 sm:p-8 bg-gray-50 text-gray-900 dark:bg-gray-950 dark:text-gray-100">
  <div class="max-w-2xl mx-auto bg-white dark:bg-gray-900 rounded-xl shadow-lg p-6 sm:p-8 ring-1 ring-black/5 dark:ring-white/10">
    <header class="mb-6">
      <div class="flex items-center justify-center gap-2 mb-4">
        <button on:click={prevDay} class="p-2 rounded-lg border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors" type="button" title="Previous day">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" /></svg>
        </button>
        <input
          type="date"
          value={workDate}
          on:change={onDateInput}
          class="px-3 py-1.5 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <button on:click={nextDay} class="p-2 rounded-lg border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors" type="button" title="Next day">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
        </button>
        {#if !isToday}
          <button on:click={goToday} class="px-3 py-1.5 text-sm font-medium rounded-lg border border-blue-300 dark:border-blue-700 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 hover:bg-blue-100 dark:hover:bg-blue-950/60 transition-colors" type="button">
            Today
          </button>
        {/if}
      </div>
      <div>
        <h1 class="text-3xl font-bold text-gray-900 dark:text-gray-100 text-center sm:text-left">Task Timer</h1>
        <p class="text-center sm:text-left text-gray-500 dark:text-gray-400 mt-1">
          {timerMode === 'parallel'
            ? 'Add tasks and track your time. Multiple timers can run at once.'
            : 'Add tasks and track your time. Only one timer runs at a time.'}
        </p>
      </div>

      <div class="hidden sm:flex flex-wrap items-center gap-x-4 gap-y-2 mt-5">
        <div class="flex items-center gap-2" role="group" aria-label="Timer modes">
          <button
            on:click={toggleTimerMode}
            class="inline-flex items-center gap-1.5 px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border transition-colors {timerMode === 'parallel'
              ? 'bg-blue-600 text-white border-blue-600 hover:bg-blue-700'
              : 'bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800'}"
            type="button"
            title="Switch between Focus (one timer at a time) and Parallel (multiple timers at once)"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
            {timerMode === 'parallel' ? 'Parallel' : 'Focus'}
          </button>
          <button
            on:click={toggleEditMode}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border transition-colors {editMode
              ? 'bg-blue-600 text-white border-blue-600 hover:bg-blue-700'
              : 'bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800'}"
            type="button"
          >
            {editMode ? 'Exit Edit Mode' : 'Edit Mode'}
          </button>
        </div>

        <div class="h-6 w-px bg-gray-200 dark:bg-gray-700" aria-hidden="true"></div>

        <button
          on:click={resetAllTimers}
          class="inline-flex items-center px-3 py-2 text-sm font-semibold text-orange-700 dark:text-orange-300 bg-orange-50 dark:bg-orange-950/40 border border-orange-200 dark:border-orange-900/60 rounded-lg shadow-sm hover:bg-orange-100 dark:hover:bg-orange-950/60 transition-colors"
          type="button"
        >
          Reset All
        </button>

        <div class="h-6 w-px bg-gray-200 dark:bg-gray-700" aria-hidden="true"></div>

        <div class="inline-flex rounded-lg shadow-sm border border-gray-300 dark:border-gray-700 overflow-hidden" role="group" aria-label="Export report">
          <button
            on:click={exportTasksAsCsv}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold border-r border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
            type="button"
          >
            CSV
          </button>
          <button
            on:click={exportTasksAsMarkdown}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
            type="button"
          >
            Markdown
          </button>
        </div>

        <div class="flex-1 min-w-4" aria-hidden="true"></div>

        <div class="flex items-center gap-2" role="group" aria-label="Account">
          <a
            href="/dashboard/archive"
            class="inline-flex items-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
          >
            Archive
          </a>
          <a
            href="/dashboard/keys"
            class="inline-flex items-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
            title="Manage API keys"
          >
            API Keys
          </a>
          <form method="POST" action="/logout" class="inline">
            <button
              type="submit"
              class="inline-flex items-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border bg-white dark:bg-gray-900 text-gray-600 dark:text-gray-400 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
              title="Sign out"
            >
              Sign Out
            </button>
          </form>
        </div>
      </div>
      <div class="mt-4 grid grid-cols-2 gap-2 sm:hidden">
        <button
          on:click={toggleTimerMode}
          class="inline-flex items-center justify-center gap-1.5 px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border transition-colors {timerMode === 'parallel'
            ? 'bg-blue-600 text-white border-blue-600 hover:bg-blue-700'
            : 'bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800'}"
          type="button"
        >
          {timerMode === 'parallel' ? 'Parallel' : 'Focus'}
        </button>
        <button
          on:click={toggleEditMode}
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border transition-colors {editMode
            ? 'bg-blue-600 text-white border-blue-600 hover:bg-blue-700'
            : 'bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800'}"
          type="button"
        >
          {editMode ? 'Exit Edit Mode' : 'Edit Mode'}
        </button>
        <button on:click={resetAllTimers} class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-orange-600 rounded-lg shadow-sm hover:bg-orange-700 transition-colors" type="button">Reset All</button>
        <button on:click={exportTasksAsCsv} class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-emerald-600 rounded-lg shadow-sm hover:bg-emerald-700 transition-colors" type="button">Export CSV</button>
        <button on:click={exportTasksAsMarkdown} class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-purple-600 rounded-lg shadow-sm hover:bg-purple-700 transition-colors" type="button">Export Markdown</button>
        <a href="/dashboard/archive" class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors">Archive</a>
        <a href="/dashboard/keys" class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold rounded-lg shadow-sm border bg-white dark:bg-gray-900 text-gray-700 dark:text-gray-200 border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors">API Keys</a>
        <form method="POST" action="/logout" class="inline">
          <button type="submit" class="w-full inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-gray-600 rounded-lg shadow-sm hover:bg-gray-700 transition-colors">Sign Out</button>
        </form>
      </div>
    </header>

    <form on:submit={addTask} class="flex flex-col sm:flex-row space-y-2 sm:space-y-0 sm:space-x-2 mb-8">
      <input
        type="text"
        bind:value={taskInput}
        placeholder="Enter new task name..."
        class="flex-1 p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400"
      />
      <button type="submit" class="p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm">Add Task</button>
    </form>

    <div class="mb-6 p-4 bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-950/30 dark:to-indigo-950/30 rounded-lg border-2 border-blue-200 dark:border-blue-900" class:animate-pulse={pulse8Hour}>
      <div class="text-center">
        <p class="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2">Total Time</p>
        <p class="text-4xl font-mono font-bold text-blue-700 dark:text-blue-300">{formatTime(totalSeconds)}</p>
      </div>
    </div>

    <div class="space-y-4">
      {#if tasks.length === 0}
        <p class="text-gray-500 dark:text-gray-400 text-center">
          {isToday ? 'No tasks added yet. Add one above to get started!' : `No tasks on ${workDate}.`}
        </p>
      {:else}
        {#each tasks as task, index (task.id)}
          {#if editMode && editBuffers[task.id]}
            <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between bg-amber-50 dark:bg-amber-950/20 p-4 rounded-lg shadow-sm border-2 border-amber-300 dark:border-amber-900/50 transition duration-200">
              <div class="flex-1 mb-3 sm:mb-0 w-full space-y-2">
                <input type="text" bind:value={editBuffers[task.id].label} on:blur={() => saveInlineTitle(task)} on:keydown={blurOnEnter} placeholder="Task title" maxlength="200" class="w-full p-2 text-lg font-medium border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" />
                <input type="text" bind:value={editBuffers[task.id].description} on:blur={() => saveInlineDescription(task)} on:keydown={blurOnEnter} placeholder="Description (optional)" class="w-full p-2 text-sm text-gray-600 dark:text-gray-300 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" />
                <input type="text" bind:value={editBuffers[task.id].time} on:blur={() => saveInlineTime(task)} on:keydown={blurOnEnter} placeholder="HH:MM:SS or minutes" class="w-full p-2 text-xl font-mono border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" />
              </div>
              <div class="flex space-x-2 w-full sm:w-auto mt-2 sm:mt-0">
                <button on:click={() => moveTaskUp(task.id)} disabled={index === 0} class="p-2 rounded-lg text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition duration-200 flex items-center justify-center {index === 0 ? 'opacity-50 cursor-not-allowed' : ''}" title="Move up">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" /></svg>
                </button>
                <button on:click={() => moveTaskDown(task.id)} disabled={index === tasks.length - 1} class="p-2 rounded-lg text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition duration-200 flex items-center justify-center {index === tasks.length - 1 ? 'opacity-50 cursor-not-allowed' : ''}" title="Move down">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" /></svg>
                </button>
                <button on:click={() => resetTimer(task.id)} class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-orange-500 hover:bg-orange-600 transition duration-200 flex items-center justify-center text-sm font-semibold" title="Reset">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
                </button>
                <button on:click={() => deleteTask(task.id, true)} class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-red-500 hover:bg-red-600 transition duration-200 flex items-center justify-center text-sm font-semibold" title="Delete (no confirmation)">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                </button>
              </div>
            </div>
          {:else}
            <div
              class="flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 dark:bg-gray-800/70 p-4 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 cursor-grab active:cursor-grabbing select-none hover:bg-gray-100 dark:hover:bg-gray-800 transition duration-200 {task.isRunning ? 'ring-2 ring-green-400 dark:ring-green-500' : ''} {draggingId === task.id ? 'opacity-60 ring-2 ring-blue-300 dark:ring-blue-500' : ''}"
              draggable={true}
              on:dragstart={(e) => handleDragStart(e, task.id)}
              on:dragover={(e) => handleDragOver(e, task.id)}
              on:drop={handleDrop}
              on:dragend={handleDragEnd}
              on:click={() => toggleTimer(task)}
              on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && toggleTimer(task)}
              role="button"
              tabindex="0"
            >
              <div class="flex-1 mb-3 sm:mb-0 flex items-start gap-3">
                <input
                  type="checkbox"
                  checked={task.done}
                  on:click|stopPropagation
                  on:change={(e) => toggleDone(task, e.currentTarget.checked)}
                  class="mt-1.5 h-5 w-5 shrink-0 rounded border-gray-300 dark:border-gray-600 text-green-600 focus:ring-green-500 cursor-pointer"
                  title="Mark done (moves the linked JIRA issue to Cek lokal)"
                />
                <div>
                  <span class="text-lg font-medium text-gray-900 dark:text-gray-100 break-words {task.done ? 'line-through text-gray-400 dark:text-gray-500' : ''}">{task.label}</span>
                  <span class="text-3xl font-mono text-gray-700 dark:text-gray-300 block mt-1">{formatTime(elapsed(task, now))}</span>
                </div>
              </div>
              <div class="flex space-x-2 w-full sm:w-auto" on:click|stopPropagation on:keydown|stopPropagation role="none">
                <button on:click={() => moveTaskUp(task.id)} disabled={index === 0} class="p-2 rounded-lg text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition duration-200 flex items-center justify-center {index === 0 ? 'opacity-50 cursor-not-allowed' : ''}" title="Move up">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" /></svg>
                </button>
                <button on:click={() => moveTaskDown(task.id)} disabled={index === tasks.length - 1} class="p-2 rounded-lg text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition duration-200 flex items-center justify-center {index === tasks.length - 1 ? 'opacity-50 cursor-not-allowed' : ''}" title="Move down">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" /></svg>
                </button>
                <button on:click={() => resetTimer(task.id)} class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-orange-500 hover:bg-orange-600 transition duration-200 flex items-center justify-center text-sm font-semibold" title="Reset">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
                </button>
                <button on:click={() => openEditModal(task)} class="w-1/3 sm:w-24 p-2 rounded-lg text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition duration-200 flex items-center justify-center text-sm font-semibold" title="Edit task">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg>
                </button>
                <button on:click={() => deleteTask(task.id)} class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-gray-400 hover:bg-gray-500 transition duration-200 flex items-center justify-center text-sm font-semibold" title="Delete">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                </button>
              </div>
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  </div>
</div>

{#if showEditModal}
  <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4" on:click={closeEditModal} on:keydown={(e) => e.key === 'Escape' && closeEditModal()} role="dialog" aria-modal="true" aria-labelledby="edit-modal-title" tabindex="-1">
    <div class="bg-white dark:bg-gray-900 rounded-lg shadow-xl max-w-md w-full p-6 ring-1 ring-black/5 dark:ring-white/10" on:click|stopPropagation on:keydown|stopPropagation role="document">
      <h2 id="edit-modal-title" class="text-2xl font-bold text-gray-900 dark:text-gray-100 mb-4">Edit Task</h2>
      <div class="space-y-4">
        <div>
          <label for="edit-title" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Task Title</label>
          <input id="edit-title" type="text" bind:value={editTitle} maxlength="200" class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" placeholder="Enter task title" />
        </div>
        <div>
          <label for="edit-description" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Description (optional)</label>
          <textarea id="edit-description" bind:value={editDescription} rows="3" class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" placeholder="Enter task description"></textarea>
        </div>
        <div>
          <label for="edit-time" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Time</label>
          <input id="edit-time" type="text" bind:value={editTime} class="w-full p-3 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400" placeholder="HH:MM:SS or minutes (e.g. 90)" />
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">Use HH:MM:SS (e.g. 01:30:00) or minutes (e.g. 90 for 1.5 hours)</p>
        </div>
      </div>
      <div class="flex justify-end gap-3 mt-6">
        <button on:click={closeEditModal} class="px-4 py-2 text-sm font-semibold text-gray-700 dark:text-gray-200 bg-gray-100 dark:bg-gray-800 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors" type="button">Cancel</button>
        <button on:click={saveEditTask} class="px-4 py-2 text-sm font-semibold text-white bg-blue-600 rounded-lg hover:bg-blue-700 transition-colors" type="button">Save</button>
      </div>
    </div>
  </div>
{/if}
