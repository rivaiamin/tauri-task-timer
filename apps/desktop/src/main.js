if (window.__TAURI__) {
  const { getCurrentWindow } = window.__TAURI__.window;
}

// when using `"withGlobalTauri": true`, you may use
// const { getCurrentWindow } = window.__TAURI__.window;

document.addEventListener("DOMContentLoaded", () => {
  const taskForm = document.getElementById("task-form");
  const taskInput = document.getElementById("task-input");
  const taskList = document.getElementById("task-list");
  const totalTimeDisplay = document.getElementById("total-time-display");
  const exportCsvButton = document.getElementById("export-csv");
  const exportCsvMobileButton = document.getElementById("export-csv-mobile");
  const exportMarkdownButton = document.getElementById("export-markdown");
  const exportMarkdownMobileButton = document.getElementById("export-markdown-mobile");
  const resetAllButton = document.getElementById("reset-all");
  const resetAllMobileButton = document.getElementById("reset-all-mobile");

  if (window.__TAURI__) {
    const appWindow = getCurrentWindow();
  }

  document
    .querySelector('[data-tauri-drag-region]')
    ?.addEventListener('dragstart', (event) => {
      event.preventDefault();
    });

  let tasks = [];
  let totalTimerInterval = null;

  function renderTasks() {
    taskList.innerHTML = "";

    if (tasks.length === 0) {
      taskList.innerHTML =
        '<p class="text-gray-500 text-center">No tasks added yet. Add one above to get started!</p>';
      return;
    }

    tasks.forEach((task) => {
      const taskElement = document.createElement("div");
      taskElement.className =
        "flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 p-4 rounded-lg shadow-sm border border-gray-200 cursor-pointer hover:bg-gray-100 transition duration-200 " +
        (task.isRunning ? "ring-2 ring-green-400" : "");
      taskElement.dataset.id = task.id;
      taskElement.dataset.action = "toggle";

      const formattedTime = formatTime(getCurrentElapsedTime(task));
      const taskIndex = tasks.findIndex(t => t.id === task.id);
      const isFirst = taskIndex === 0;
      const isLast = taskIndex === tasks.length - 1;

      taskElement.innerHTML = `
        <div class="flex-1 mb-3 sm:mb-0">
          <span class="text-lg font-medium text-gray-900 break-words">${escapeHTML(
            task.label,
          )}</span>
          <span id="time-${task.id}" class="text-3xl font-mono text-gray-700 block mt-1">${formattedTime}</span>
        </div>
        <div class="flex space-x-2 w-full sm:w-auto">
          <button
            data-id="${task.id}"
            data-action="move-up"
            class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold ${isFirst ? 'opacity-50 cursor-not-allowed' : ''}"
            title="Move up"
            ${isFirst ? 'disabled' : ''}
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
            </svg>
          </button>
          <button
            data-id="${task.id}"
            data-action="move-down"
            class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold ${isLast ? 'opacity-50 cursor-not-allowed' : ''}"
            title="Move down"
            ${isLast ? 'disabled' : ''}
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
          </button>
          <button
            data-id="${task.id}"
            data-action="reset"
            class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-orange-500 hover:bg-orange-600 transition duration-200 flex items-center justify-center text-sm font-semibold"
            title="Reset"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
          <button
            data-id="${task.id}"
            data-action="edit"
            class="w-1/3 sm:w-24 p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold"
            title="Edit task"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </button>
          <button
            data-id="${task.id}"
            data-action="delete"
            class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-gray-400 hover:bg-gray-500 transition duration-200 flex items-center justify-center text-sm font-semibold"
            title="Delete"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
      `;
      taskList.appendChild(taskElement);
    });

    // Update total timer after rendering
    updateTotalTimer();
  }

  function moveTaskUp(id) {
    const index = tasks.findIndex((task) => task.id === id);
    if (index <= 0) return; // Already at the top

    // Swap with the task above
    [tasks[index - 1], tasks[index]] = [tasks[index], tasks[index - 1]];
    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  function moveTaskDown(id) {
    const index = tasks.findIndex((task) => task.id === id);
    if (index < 0 || index >= tasks.length - 1) return; // Already at the bottom

    // Swap with the task below
    [tasks[index], tasks[index + 1]] = [tasks[index + 1], tasks[index]];
    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  function toggleTimer(id) {
    const taskToToggle = tasks.find((task) => task.id === id);
    if (!taskToToggle) return;

    const isStarting = !taskToToggle.isRunning;

    if (isStarting) {
      tasks.forEach((task) => {
        if (task.isRunning) {
          // Save accumulated time before stopping
          task.elapsedTime = getCurrentElapsedTime(task);
          clearInterval(task.intervalId);
          task.isRunning = false;
          task.intervalId = null;
          task.startTime = null;
        }
      });
    }

    if (isStarting) {
      taskToToggle.isRunning = true;
      taskToToggle.startTime = Date.now();
      
      // Update display immediately
      updateTimerDisplay(taskToToggle);
      
      taskToToggle.intervalId = setInterval(() => {
        updateTimerDisplay(taskToToggle);
        saveTasks();
      }, 1000);

      // Start total timer interval if not already running
      if (!totalTimerInterval) {
        totalTimerInterval = setInterval(() => {
          updateTotalTimer();
        }, 1000);
      }
    } else {
      // Save accumulated time before stopping
      taskToToggle.elapsedTime = getCurrentElapsedTime(taskToToggle);
      clearInterval(taskToToggle.intervalId);
      taskToToggle.isRunning = false;
      taskToToggle.intervalId = null;
      taskToToggle.startTime = null;

      // Stop total timer interval if no timers are running
      const hasRunningTimer = tasks.some(task => task.isRunning);
      if (!hasRunningTimer && totalTimerInterval) {
        clearInterval(totalTimerInterval);
        totalTimerInterval = null;
        // Final update
        updateTotalTimer();
      }
    }

    saveTasks();
    renderTasks();
  }

  function getCurrentElapsedTime(task) {
    if (!task.isRunning || !task.startTime) {
      return task.elapsedTime;
    }
    const elapsedSinceStart = Math.floor((Date.now() - task.startTime) / 1000);
    return task.elapsedTime + elapsedSinceStart;
  }

  function updateTimerDisplay(task) {
    const currentElapsed = getCurrentElapsedTime(task);
    const timeElement = document.getElementById(`time-${task.id}`);
    if (timeElement) {
      timeElement.textContent = formatTime(currentElapsed);
    }
    updateTotalTimer();
  }

  function getTotalTime() {
    return tasks.reduce((total, task) => {
      return total + getCurrentElapsedTime(task);
    }, 0);
  }

  let hasPlayed8HourSound = false;

  function updateTotalTimer() {
    const totalSeconds = getTotalTime();
    const formattedTotal = formatTime(totalSeconds);
    
    if (totalTimeDisplay) {
      totalTimeDisplay.textContent = formattedTotal;
      
      // Check if total reaches 8 hours (28800 seconds) and play sound
      if (totalSeconds >= 28800 && !hasPlayed8HourSound) {
        play8HourSound();
        hasPlayed8HourSound = true;
      } else if (totalSeconds < 28800) {
        // Reset the flag if total goes below 8 hours (e.g., after reset)
        hasPlayed8HourSound = false;
      }
    }
  }

  function play8HourSound() {
    // Create audio context for beep sound
    const audioContext = new (window.AudioContext || window.webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    // Set up a pleasant beep sound
    oscillator.frequency.value = 800; // 800 Hz
    oscillator.type = 'sine';
    
    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.5);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.5);

    // Also show a visual notification
    if (totalTimeDisplay) {
      totalTimeDisplay.classList.add('animate-pulse');
      setTimeout(() => {
        totalTimeDisplay.classList.remove('animate-pulse');
      }, 2000);
    }
  }

  function resetTimer(id) {
    const taskToReset = tasks.find((task) => task.id === id);
    if (!taskToReset) return;

    // Stop the timer if it's running
    if (taskToReset.isRunning) {
      clearInterval(taskToReset.intervalId);
      taskToReset.isRunning = false;
      taskToReset.intervalId = null;
      taskToReset.startTime = null;
    }

    // Reset elapsed time to 0
    taskToReset.elapsedTime = 0;
    
    // Update the display immediately
    const timeElement = document.getElementById(`time-${taskToReset.id}`);
    if (timeElement) {
      timeElement.textContent = formatTime(0);
    }

    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  async function resetAllTimers() {
    if (tasks.length === 0) {
      alert("No tasks to reset.");
      return;
    }

    if (!confirm("Are you sure you want to reset all timers to 00:00:00?")) {
      return;
    }

    // Stop all running timers
    tasks.forEach((task) => {
      if (task.isRunning) {
        clearInterval(task.intervalId);
        task.isRunning = false;
        task.intervalId = null;
        task.startTime = null;
      }
      // Reset elapsed time to 0
      task.elapsedTime = 0;
    });

    // Stop total timer interval
    if (totalTimerInterval) {
      clearInterval(totalTimerInterval);
      totalTimerInterval = null;
    }

    // Reset in database if using Tauri
    if (window.__TAURI__) {
      try {
        await window.__TAURI__.core.invoke("reset_all_tasks");
      } catch (error) {
        console.error("Failed to reset tasks in database:", error);
      }
    }

    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  function parseTimeInput(value) {
    const input = String(value ?? "").trim();
    if (!input) return null;

    // Support HH:MM:SS, MM:SS, or SS
    if (/^\d+(:\d+){0,2}$/.test(input)) {
      const parts = input.split(":").map((part) => Number(part));
      if (parts.some((n) => Number.isNaN(n) || n < 0)) return null;

      let hours = 0;
      let minutes = 0;
      let seconds = 0;

      if (parts.length === 3) {
        [hours, minutes, seconds] = parts;
      } else if (parts.length === 2) {
        [minutes, seconds] = parts;
      } else {
        [seconds] = parts;
      }

      return hours * 3600 + minutes * 60 + seconds;
    }

    // Fallback: treat a plain number as minutes (can be decimal)
    const asNumber = Number(input.replace(",", "."));
    if (!Number.isFinite(asNumber) || asNumber < 0) return null;
    return Math.round(asNumber * 60);
  }

  async function editTask(id) {
    const task = tasks.find((t) => t.id === id);
    if (!task) return;

    const currentSeconds = getCurrentElapsedTime(task);
    const currentFormatted = formatTime(currentSeconds);

    const { value: formValues } = await Swal.fire({
      title: 'Edit Task',
      html: `
        <div class="text-left">
          <label for="swal-task-title" class="block text-sm font-medium text-gray-700 mb-2">Task Title</label>
          <input 
            id="swal-task-title" 
            class="swal2-input w-full mb-4" 
            placeholder="Enter task title" 
            value="${escapeHTML(task.label)}"
            maxlength="200"
          />
          <label for="swal-task-time" class="block text-sm font-medium text-gray-700 mb-2">Time</label>
          <input 
            id="swal-task-time" 
            class="swal2-input w-full" 
            placeholder="HH:MM:SS or minutes (e.g. 90)" 
            value="${currentFormatted}"
          />
          <p class="text-xs text-gray-500 mt-2">Use HH:MM:SS (e.g. 01:30:00) or minutes (e.g. 90 for 1.5 hours)</p>
        </div>
      `,
      focusConfirm: false,
      showCancelButton: true,
      confirmButtonText: 'Save',
      cancelButtonText: 'Cancel',
      confirmButtonColor: '#2563eb',
      cancelButtonColor: '#6b7280',
      preConfirm: () => {
        const titleInput = document.getElementById('swal-task-title');
        const timeInput = document.getElementById('swal-task-time');
        
        const newTitle = titleInput.value.trim();
        const timeValue = timeInput.value.trim();
        
        if (!newTitle) {
          Swal.showValidationMessage('Task title cannot be empty');
          return false;
        }
        
        if (!timeValue) {
          Swal.showValidationMessage('Time cannot be empty');
          return false;
        }
        
        const newSeconds = parseTimeInput(timeValue);
        if (newSeconds === null) {
          Swal.showValidationMessage('Invalid time format. Use HH:MM:SS or minutes (e.g. 90)');
          return false;
        }
        
        return {
          title: newTitle,
          time: newSeconds
        };
      }
    });

    if (!formValues) {
      // User cancelled
      return;
    }

    // Update task title
    task.label = formValues.title;

    // Update stored elapsed time based on new value
    task.elapsedTime = formValues.time;

    // If the timer is running, keep it running from the new base time
    if (task.isRunning) {
      task.startTime = Date.now();
    }

    saveTasks();
    renderTasks();
  }

  async function deleteTask(id) {
    const taskToDelete = tasks.find((task) => task.id === id);
    if (taskToDelete && taskToDelete.isRunning) {
      clearInterval(taskToDelete.intervalId);
    }

    tasks = tasks.filter((task) => task.id !== id);
    
    // Delete from database if using Tauri
    if (window.__TAURI__) {
      try {
        await window.__TAURI__.core.invoke("delete_task", { id });
      } catch (error) {
        console.error("Failed to delete task from database:", error);
      }
    }
    
    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  function addTask(event) {
    event.preventDefault();
    const label = taskInput.value.trim();
    if (!label) return;

    const newTask = {
      id: Date.now(),
      label,
      elapsedTime: 0,
      isRunning: false,
      intervalId: null,
      startTime: null,
    };

    tasks.push(newTask);
    taskInput.value = "";
    saveTasks();
    renderTasks();
    updateTotalTimer();
  }

  async function saveTasks() {
    const tasksToSave = tasks.map((task) => ({
      id: task.id,
      label: task.label,
      elapsedTime: task.elapsedTime,
    }));

    // Use Tauri database if available, otherwise fallback to localStorage
    if (window.__TAURI__) {
      try {
        await window.__TAURI__.core.invoke("save_tasks", {
          tasks: tasksToSave.map((task) => ({
            id: task.id,
            label: task.label,
            elapsed_time: task.elapsedTime,
            position: 0, // Will be set by Rust based on array index
          })),
        });
      } catch (error) {
        console.error("Failed to save tasks to database:", error);
        // Fallback to localStorage on error
        localStorage.setItem("taskTimerApp", JSON.stringify(tasksToSave));
      }
    } else {
      // Fallback for web/dev mode
      localStorage.setItem("taskTimerApp", JSON.stringify(tasksToSave));
    }
  }

  async function loadTasks() {
    // Use Tauri database if available, otherwise fallback to localStorage
    if (window.__TAURI__) {
      try {
        const dbTasks = await window.__TAURI__.core.invoke("get_tasks");
        return dbTasks.map((task) => ({
          id: task.id,
          label: task.label,
          elapsedTime: task.elapsed_time,
          isRunning: false,
          intervalId: null,
          startTime: null,
        }));
      } catch (error) {
        console.error("Failed to load tasks from database:", error);
        // Fallback to localStorage on error
        return loadTasksFromLocalStorage();
      }
    } else {
      // Fallback for web/dev mode
      return loadTasksFromLocalStorage();
    }
  }

  function loadTasksFromLocalStorage() {
    const savedTasks = localStorage.getItem("taskTimerApp");
    if (savedTasks) {
      return JSON.parse(savedTasks).map((task) => ({
        ...task,
        isRunning: false,
        intervalId: null,
        startTime: null,
      }));
    }
    return [];
  }

  function formatTime(totalSeconds) {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    return [hours, minutes, seconds]
      .map((value) => String(value).padStart(2, "0"))
      .join(":");
  }

  function escapeHTML(str) {
    const div = document.createElement("div");
    div.appendChild(document.createTextNode(str));
    return div.innerHTML;
  }

  function tasksToCsv() {
    const header = ["Task", "Story Points"];
    const rows = [header];

    tasks.forEach((task) => {
      const storyPoints = task.elapsedTime / 3600; // 60 minutes => 1 story point
      rows.push([task.label, storyPoints.toFixed(2)]);
    });

    return rows
      .map((row) =>
        row
          .map((field) => {
            const value = String(field ?? "");
            const escaped = value.replace(/"/g, '""');
            return `"${escaped}"`;
          })
          .join(","),
      )
      .join("\r\n");
  }

  function tasksToMarkdown() {
    // Template format for Google Chat compatibility
    const datePart = new Date().toISOString().slice(0, 10); // YYYY-MM-DD
    const lines = [`### Daily Report ${datePart}`];
    
    tasks.forEach((task) => {
      const storyPoints = getCurrentElapsedTime(task) / 3600; // 60 minutes => 1 story point
      lines.push(`- [${storyPoints.toFixed(2)}] ${task.label}`);
    });
    
    return lines.join("\n");
  }

  async function exportTasksAsCsv() {
    if (!tasks.length) {
      alert("No tasks to export yet.");
      return;
    }

    const csvContent = tasksToCsv();
    const datePart = new Date().toISOString().slice(0, 10);

    // Prefer Tauri-native save dialog when available
    const tauri = window.__TAURI__;

    if (tauri && tauri.fs && tauri.dialog) {
      try {
        const filePath = await tauri.dialog.save({
          defaultPath: `tasks-${datePart}.csv`,
          filters: [
            {
              name: "CSV Files",
              extensions: ["csv"],
            },
          ],
        });

        // User cancelled the dialog
        if (!filePath) {
          return;
        }

        await tauri.fs.writeTextFile(filePath, csvContent);
        alert("Tasks exported successfully.");
        return;
      } catch (error) {
        console.error("Failed to export CSV via Tauri:", error);
        alert("Failed to export CSV. Please try again.");
        return;
      }
    }

    // Fallback: browser-style download (for web/dev)
    const blob = new Blob([csvContent], {
      type: "text/csv;charset=utf-8;",
    });
    const url = URL.createObjectURL(blob);

    const link = document.createElement("a");
    link.href = url;
    link.download = `tasks-${datePart}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }

  taskForm.addEventListener("submit", addTask);

  resetAllButton?.addEventListener("click", resetAllTimers);
  resetAllMobileButton?.addEventListener("click", resetAllTimers);
  exportCsvButton?.addEventListener("click", exportTasksAsCsv);
  exportCsvMobileButton?.addEventListener("click", exportTasksAsCsv);
  exportMarkdownButton?.addEventListener("click", tasksToMarkdown);
  exportMarkdownMobileButton?.addEventListener("click", tasksToMarkdown);

  taskList.addEventListener("click", (event) => {
    // Check if a button was clicked (stop propagation to prevent card toggle)
    const button = event.target.closest("button");
    if (button) {
      event.stopPropagation();
      const id = Number(button.dataset.id);
      const action = button.dataset.action;

      if (action === "move-up") {
        moveTaskUp(id);
      } else if (action === "move-down") {
        moveTaskDown(id);
      } else if (action === "reset") {
        resetTimer(id);
      } else if (action === "edit") {
        editTask(id);
      } else if (action === "delete") {
        if (confirm("Are you sure you want to delete this task?")) {
          deleteTask(id);
        }
      }
      return;
    }

    // Check if the card itself was clicked (but not a button)
    const card = event.target.closest("[data-action='toggle']");
    if (card && !event.target.closest("button")) {
      const id = Number(card.dataset.id);
      toggleTimer(id);
    }
  });

  // Update timers when page becomes visible (handles background tab issue)
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      // Page became visible - update all running timers
      tasks.forEach((task) => {
        if (task.isRunning) {
          updateTimerDisplay(task);
        }
      });
      updateTotalTimer();
    }
  });

  // Load tasks and then render
  (async () => {
    tasks = await loadTasks();
    renderTasks();

    // Start total timer interval if any timers are already running (e.g., after page reload)
    const hasRunningTimer = tasks.some(task => task.isRunning);
    if (hasRunningTimer && !totalTimerInterval) {
      totalTimerInterval = setInterval(() => {
        updateTotalTimer();
      }, 1000);
    }
  })();
});
