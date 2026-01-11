<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { supabase } from '$lib/supabaseClient';
  import { formatTime } from 'shared';
  import type { DatabaseTask } from 'shared';
  import { getSession, signOut } from '$lib/auth/helpers';
  import { goto } from '$app/navigation';

  let session: any = null;
  let tasks: (DatabaseTask & { intervalId?: any; startTime?: number | null })[] = [];
  let totalTime = 0;
  let taskInput = '';
  let totalTimerInterval: ReturnType<typeof setInterval> | null = null;
  let hasPlayed8HourSound = false;
  let channel: any = null;
  let tick = 0; // Reactive variable to force timer updates every second
  
  // Edit modal state
  let showEditModal = false;
  let editingTask: (DatabaseTask & { intervalId?: any; startTime?: number | null }) | null = null;
  let editTitle = '';
  let editDescription = '';
  let editTime = '';

  // Reactive: Update total time whenever tasks change
  $: {
    totalTime = getTotalTime();
    check8HourNotification();
  }

  onMount(async () => {
    session = await getSession();
    if (!session) return;

    await loadTasks();

    // Subscribe to real-time updates
    channel = supabase
      .channel('tasks-changes')
      .on(
        'postgres_changes',
        {
          event: '*',
          schema: 'public',
          table: 'tasks',
          filter: `user_id=eq.${session.user.id}`
        },
        (payload: any) => {
          handleRealtimeUpdate(payload);
        }
      )
      .subscribe();

    // Start total timer interval - updates tick to force re-renders
    totalTimerInterval = setInterval(() => {
      tick = Date.now(); // Update tick to trigger reactivity for all timers
      totalTime = getTotalTime();
    }, 1000);

    // Handle visibility change (for background tab issue)
    document.addEventListener('visibilitychange', handleVisibilityChange);
  });

  onDestroy(() => {
    if (channel) {
      supabase.removeChannel(channel);
    }
    if (totalTimerInterval) {
      clearInterval(totalTimerInterval);
    }
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  });

  async function loadTasks() {
    if (!session) return;

    const { data, error } = await supabase
      .from('tasks')
      .select('*')
      .eq('user_id', session.user.id)
      .order('position', { ascending: true });

    if (error) {
      console.error('Error loading tasks:', error);
      return;
    }

    tasks = (data || []).map(task => ({
      ...task,
      intervalId: null,
      startTime: task.start_time ? new Date(task.start_time).getTime() : null
    }));
  }

  function handleRealtimeUpdate(payload: any) {
    if (payload.eventType === 'INSERT') {
      tasks = [...tasks, {
        ...payload.new,
        intervalId: null,
        startTime: payload.new.start_time ? new Date(payload.new.start_time).getTime() : null
      }];
    } else if (payload.eventType === 'UPDATE') {
      tasks = tasks.map(task => 
        task.id === payload.new.id 
          ? {
              ...payload.new,
              intervalId: task.intervalId,
              startTime: payload.new.start_time ? new Date(payload.new.start_time).getTime() : task.startTime
            }
          : task
      );
    } else if (payload.eventType === 'DELETE') {
      tasks = tasks.filter(t => t.id !== payload.old.id);
    }
  }

  function getCurrentElapsedTime(task: DatabaseTask & { startTime?: number | null }): number {
    if (!task.is_running || !task.startTime) {
      return task.elapsed_time;
    }
    const elapsedSinceStart = Math.floor((Date.now() - task.startTime) / 1000);
    return task.elapsed_time + elapsedSinceStart;
  }

  function getTotalTime(): number {
    return tasks.reduce((sum, task) => {
      return sum + getCurrentElapsedTime(task);
    }, 0);
  }

  function check8HourNotification() {
    if (totalTime >= 28800 && !hasPlayed8HourSound) {
      play8HourSound();
      hasPlayed8HourSound = true;
    } else if (totalTime < 28800) {
      hasPlayed8HourSound = false;
    }
  }

  function play8HourSound() {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    oscillator.frequency.value = 800;
    oscillator.type = 'sine';
    
    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.5);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.5);
  }

  async function addTask(event: Event) {
    event.preventDefault();
    const label = taskInput.trim();
    if (!label || !session) return;

    const { data, error } = await supabase
      .from('tasks')
      .insert({
        user_id: session.user.id,
        label,
        description: null,
        elapsed_time: 0,
        position: tasks.length,
        is_running: false
      })
      .select()
      .single();

    if (error) {
      console.error('Error adding task:', error);
      alert('Failed to add task');
      return;
    }

    tasks = [...tasks, {
      ...data,
      intervalId: null,
      startTime: null
    }];
    taskInput = '';
  }

  async function toggleTimer(id: number) {
    if (!session) return;

    const taskToToggle = tasks.find(t => t.id === id);
    if (!taskToToggle) return;

    const isStarting = !taskToToggle.is_running;

    // Stop all other running timers
    if (isStarting) {
      for (const task of tasks) {
        if (task.is_running && task.id !== id) {
          await stopTimer(task.id);
        }
      }
    }

    if (isStarting) {
      await startTimer(id);
    } else {
      await stopTimer(id);
    }
  }

  async function startTimer(id: number) {
    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        is_running: true,
        start_time: new Date().toISOString()
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error starting timer:', error);
      return;
    }

    // Update local state
    tasks = tasks.map(task => 
      task.id === id 
        ? { ...task, is_running: true, startTime: Date.now() }
        : task
    );
  }

  async function stopTimer(id: number) {
    if (!session) return;

    const task = tasks.find(t => t.id === id);
    if (!task) return;

    const currentElapsed = getCurrentElapsedTime(task);

    const { error } = await supabase
      .from('tasks')
      .update({
        is_running: false,
        elapsed_time: currentElapsed,
        start_time: null
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error stopping timer:', error);
      return;
    }

    // Update local state
    tasks = tasks.map(t => 
      t.id === id 
        ? { ...t, is_running: false, elapsed_time: currentElapsed, startTime: null }
        : t
    );
  }

  async function resetTimer(id: number) {
    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        elapsed_time: 0,
        is_running: false,
        start_time: null
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error resetting timer:', error);
      return;
    }

    tasks = tasks.map(t => 
      t.id === id 
        ? { ...t, elapsed_time: 0, is_running: false, startTime: null }
        : t
    );
  }

  async function deleteTask(id: number) {
    if (!session) return;
    if (!confirm('Are you sure you want to delete this task?')) return;

    const { error } = await supabase
      .from('tasks')
      .delete()
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error deleting task:', error);
      return;
    }

    tasks = tasks.filter(t => t.id !== id);
  }

  async function moveTaskUp(id: number) {
    const index = tasks.findIndex(t => t.id === id);
    if (index <= 0 || !session) return;

    const newPosition = index - 1;
    await reorderTasks(id, newPosition);
  }

  async function moveTaskDown(id: number) {
    const index = tasks.findIndex(t => t.id === id);
    if (index < 0 || index >= tasks.length - 1 || !session) return;

    const newPosition = index + 1;
    await reorderTasks(id, newPosition);
  }

  async function reorderTasks(taskId: number, newPosition: number) {
    if (!session) return;

    // Swap positions
    const task1 = tasks[newPosition];
    const task2 = tasks.find(t => t.id === taskId);
    if (!task2) return;

    // Update both tasks' positions
    const updates = [
      supabase.from('tasks').update({ position: newPosition }).eq('id', taskId).eq('user_id', session.user.id),
      supabase.from('tasks').update({ position: task1.position }).eq('id', task1.id).eq('user_id', session.user.id)
    ];

    await Promise.all(updates);

    // Update local state
    const newTasks = [...tasks];
    const task2Index = tasks.findIndex(t => t.id === taskId);
    [newTasks[newPosition], newTasks[task2Index]] = [newTasks[task2Index], newTasks[newPosition]];
    tasks = newTasks;
  }

  function parseTimeInput(value: string): number | null {
    const input = value.trim();
    if (!input) return null;

    // Support HH:MM:SS, MM:SS, or SS
    if (/^\d+(:\d+){0,2}$/.test(input)) {
      const parts = input.split(':').map(Number);
      if (parts.some(n => isNaN(n) || n < 0)) return null;

      let hours = 0, minutes = 0, seconds = 0;
      if (parts.length === 3) {
        [hours, minutes, seconds] = parts;
      } else if (parts.length === 2) {
        [minutes, seconds] = parts;
      } else {
        [seconds] = parts;
      }

      return hours * 3600 + minutes * 60 + seconds;
    }

    // Fallback: treat as minutes (can be decimal)
    const asNumber = Number(input.replace(',', '.'));
    if (!isFinite(asNumber) || asNumber < 0) return null;
    return Math.round(asNumber * 60);
  }

  function openEditModal(id: number) {
    const task = tasks.find(t => t.id === id);
    if (!task) return;

    editingTask = task;
    editTitle = task.label;
    editDescription = task.description || '';
    const currentSeconds = getCurrentElapsedTime(task);
    editTime = formatTime(currentSeconds);
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
    if (!session || !editingTask) return;

    const newTitle = editTitle.trim();
    if (!newTitle) {
      alert('Task title cannot be empty');
      return;
    }

    const newSeconds = parseTimeInput(editTime);
    if (newSeconds === null) {
      alert("Invalid time format. Use HH:MM:SS or minutes (e.g. 90)");
      return;
    }

    const { error } = await supabase
      .from('tasks')
      .update({
        label: newTitle,
        description: editDescription.trim() || null,
        elapsed_time: newSeconds,
        start_time: editingTask.is_running ? new Date().toISOString() : null
      })
      .eq('id', editingTask.id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error updating task:', error);
      alert('Failed to update task');
      return;
    }

    tasks = tasks.map(t => 
      t.id === editingTask!.id 
        ? { 
            ...t, 
            label: newTitle,
            description: editDescription.trim() || null,
            elapsed_time: newSeconds, 
            startTime: t.is_running ? Date.now() : null 
          }
        : t
    );

    closeEditModal();
  }

  async function resetAllTimers() {
    if (tasks.length === 0) {
      alert('No tasks to reset.');
      return;
    }

    if (!confirm('Are you sure you want to reset all timers to 00:00:00?')) {
      return;
    }

    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        elapsed_time: 0,
        is_running: false,
        start_time: null
      })
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error resetting all timers:', error);
      return;
    }

    tasks = tasks.map(t => ({
      ...t,
      elapsed_time: 0,
      is_running: false,
      startTime: null
    }));
  }

  function tasksToCsv(): string {
    const header = ['Task', 'Description', 'Story Points'];
    const rows = [header];

    tasks.forEach(task => {
      const storyPoints = getCurrentElapsedTime(task) / 3600;
      rows.push([task.label, task.description || '', storyPoints.toFixed(2)]);
    });

    return rows
      .map(row =>
        row
          .map(field => {
            const value = String(field ?? '');
            const escaped = value.replace(/"/g, '""');
            return `"${escaped}"`;
          })
          .join(',')
      )
      .join('\r\n');
  }

  function exportTasksAsCsv() {
    if (!tasks.length) {
      alert('No tasks to export yet.');
      return;
    }

    const csvContent = tasksToCsv();
    const datePart = new Date().toISOString().slice(0, 10);
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);

    const link = document.createElement('a');
    link.href = url;
    link.download = `tasks-${datePart}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }

  function tasksToMarkdown(): string {
    const datePart = new Date().toISOString().slice(0, 10);
    const lines = [`### Daily Report ${datePart}`];
    
    tasks.forEach(task => {
      const storyPoints = getCurrentElapsedTime(task) / 3600;
      let line = `- [${storyPoints.toFixed(2)}] ${task.label}`;
      if (task.description && task.description.trim()) {
        line += `\n  - ${task.description.trim()}`;
      }
      lines.push(line);
    });
    
    return lines.join('\n');
  }

  async function exportTasksAsMarkdown() {
    if (!tasks.length) {
      alert('No tasks to export yet.');
      return;
    }

    const markdownContent = tasksToMarkdown();

    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(markdownContent);
        alert('Daily report copied to clipboard!');
      } else {
        const textArea = document.createElement('textarea');
        textArea.value = markdownContent;
        textArea.style.position = 'fixed';
        textArea.style.left = '-999999px';
        document.body.appendChild(textArea);
        textArea.select();
        document.execCommand('copy');
        document.body.removeChild(textArea);
        alert('Daily report copied to clipboard!');
      }
    } catch (error) {
      console.error('Failed to copy to clipboard:', error);
      alert('Failed to copy to clipboard. Please try again.');
    }
  }

  function handleVisibilityChange() {
    if (!document.hidden) {
      // Page became visible - update all running timers
      tasks = tasks.map(task => task); // Trigger reactivity
    }
  }

  async function handleLogout() {
    const { error } = await signOut();
    if (error) {
      console.error('Error signing out:', error);
      alert('Failed to sign out');
    } else {
      goto('/login');
    }
  }
</script>

<div class="min-h-screen p-4 sm:p-8">
  <div class="max-w-2xl mx-auto bg-white rounded-xl shadow-lg p-6 sm:p-8">
    <header class="mb-6">
      <div class="flex items-center justify-between">
        <div class="flex-1">
          <h1 class="text-3xl font-bold text-gray-900 text-center sm:text-left">
            Task Timer
          </h1>
          <p class="text-center sm:text-left text-gray-500 mt-1">
            Add tasks and track your time. Only one timer can run at a time.
          </p>
        </div>
        <div class="hidden sm:flex items-center gap-2 ml-4">
          <button
            on:click={resetAllTimers}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-orange-600 rounded-lg shadow-sm hover:bg-orange-700 transition-colors"
            type="button"
          >
            Reset All
          </button>
          <button
            on:click={exportTasksAsCsv}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-emerald-600 rounded-lg shadow-sm hover:bg-emerald-700 transition-colors"
            type="button"
          >
            Export CSV
          </button>
          <button
            on:click={exportTasksAsMarkdown}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-purple-600 rounded-lg shadow-sm hover:bg-purple-700 transition-colors"
            type="button"
          >
            Export Markdown
          </button>
          <button
            on:click={handleLogout}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-gray-600 rounded-lg shadow-sm hover:bg-gray-700 transition-colors"
            type="button"
            title="Sign out"
          >
            Sign Out
          </button>
        </div>
      </div>
      <div class="mt-4 flex gap-2 sm:hidden">
        <button
          on:click={resetAllTimers}
          class="flex-1 inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-orange-600 rounded-lg shadow-sm hover:bg-orange-700 transition-colors"
          type="button"
        >
          Reset All
        </button>
        <button
          on:click={exportTasksAsCsv}
          class="flex-1 inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-emerald-600 rounded-lg shadow-sm hover:bg-emerald-700 transition-colors"
          type="button"
        >
          Export CSV
        </button>
        <button
          on:click={exportTasksAsMarkdown}
          class="flex-1 inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-purple-600 rounded-lg shadow-sm hover:bg-purple-700 transition-colors"
          type="button"
        >
          Export Markdown
        </button>
        <button
          on:click={handleLogout}
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-gray-600 rounded-lg shadow-sm hover:bg-gray-700 transition-colors"
          type="button"
          title="Sign out"
        >
          <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
          </svg>
        </button>
      </div>
    </header>

    <form on:submit={addTask} class="flex flex-col sm:flex-row space-y-2 sm:space-y-0 sm:space-x-2 mb-8">
      <input
        type="text"
        bind:value={taskInput}
        placeholder="Enter new task name..."
        class="flex-1 p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      <button
        type="submit"
        class="p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm"
      >
        Add Task
      </button>
    </form>

    <div class="mb-6 p-4 bg-gradient-to-r from-blue-50 to-indigo-50 rounded-lg border-2 border-blue-200">
      <div class="text-center">
        <p class="text-sm font-semibold text-gray-600 mb-2">Total Time</p>
        <p class="text-4xl font-mono font-bold text-blue-700">{formatTime(totalTime)}</p>
      </div>
    </div>

    <div class="space-y-4">
      {#if tasks.length === 0}
        <p class="text-gray-500 text-center">
          No tasks added yet. Add one above to get started!
        </p>
      {:else}
        {#each tasks as task (task.id), index}
          <div
            class="flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 p-4 rounded-lg shadow-sm border border-gray-200 cursor-pointer hover:bg-gray-100 transition duration-200 {task.is_running ? 'ring-2 ring-green-400' : ''}"
            on:click={() => toggleTimer(task.id)}
            on:keydown={(e) => e.key === 'Enter' || e.key === ' ' ? toggleTimer(task.id) : null}
            role="button"
            tabindex="0"
          >
            <div class="flex-1 mb-3 sm:mb-0">
              <span class="text-lg font-medium text-gray-900 break-words">{task.label}</span>
              <span class="text-3xl font-mono text-gray-700 block mt-1">
                {formatTime(getCurrentElapsedTime(task) + (tick ? 0 : 0))}
              </span>
            </div>
            <div 
              class="flex space-x-2 w-full sm:w-auto" 
              on:click|stopPropagation
              on:keydown|stopPropagation
              role="none"
            >
              <button
                on:click={() => moveTaskUp(task.id)}
                disabled={index === 0}
                class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold {index === 0 ? 'opacity-50 cursor-not-allowed' : ''}"
                title="Move up"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                </svg>
              </button>
              <button
                on:click={() => moveTaskDown(task.id)}
                disabled={index === tasks.length - 1}
                class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold {index === tasks.length - 1 ? 'opacity-50 cursor-not-allowed' : ''}"
                title="Move down"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>
              <button
                on:click={() => resetTimer(task.id)}
                class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-orange-500 hover:bg-orange-600 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Reset"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </button>
              <button
                on:click={() => openEditModal(task.id)}
                class="w-1/3 sm:w-24 p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Edit task"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
              </button>
              <button
                on:click={() => deleteTask(task.id)}
                class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-gray-400 hover:bg-gray-500 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Delete"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

{#if showEditModal}
  <div 
    class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50 p-4"
    on:click={closeEditModal}
    on:keydown={(e) => e.key === 'Escape' ? closeEditModal() : null}
    role="dialog"
    aria-modal="true"
    aria-labelledby="edit-modal-title"
  >
    <div 
      class="bg-white rounded-lg shadow-xl max-w-md w-full p-6"
      on:click|stopPropagation
    >
      <h2 id="edit-modal-title" class="text-2xl font-bold text-gray-900 mb-4">Edit Task</h2>
      
      <div class="space-y-4">
        <div>
          <label for="edit-title" class="block text-sm font-medium text-gray-700 mb-2">
            Task Title
          </label>
          <input
            id="edit-title"
            type="text"
            bind:value={editTitle}
            class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
            placeholder="Enter task title"
            maxlength="200"
          />
        </div>
        
        <div>
          <label for="edit-description" class="block text-sm font-medium text-gray-700 mb-2">
            Description (optional)
          </label>
          <textarea
            id="edit-description"
            bind:value={editDescription}
            class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
            placeholder="Enter task description"
            rows="3"
          />
        </div>
        
        <div>
          <label for="edit-time" class="block text-sm font-medium text-gray-700 mb-2">
            Time
          </label>
          <input
            id="edit-time"
            type="text"
            bind:value={editTime}
            class="w-full p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
            placeholder="HH:MM:SS or minutes (e.g. 90)"
          />
          <p class="text-xs text-gray-500 mt-2">
            Use HH:MM:SS (e.g. 01:30:00) or minutes (e.g. 90 for 1.5 hours)
          </p>
        </div>
      </div>
      
      <div class="flex justify-end gap-3 mt-6">
        <button
          on:click={closeEditModal}
          class="px-4 py-2 text-sm font-semibold text-gray-700 bg-gray-100 rounded-lg hover:bg-gray-200 transition-colors"
          type="button"
        >
          Cancel
        </button>
        <button
          on:click={saveEditTask}
          class="px-4 py-2 text-sm font-semibold text-white bg-blue-600 rounded-lg hover:bg-blue-700 transition-colors"
          type="button"
        >
          Save
        </button>
      </div>
    </div>
  </div>
{/if}
