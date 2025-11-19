const { getCurrentWindow } = window.__TAURI__.window;
// when using `"withGlobalTauri": true`, you may use
// const { getCurrentWindow } = window.__TAURI__.window;

document.addEventListener("DOMContentLoaded", () => {
  const taskForm = document.getElementById("task-form");
  const taskInput = document.getElementById("task-input");
  const taskList = document.getElementById("task-list");
  const exportCsvButton = document.getElementById("export-csv");
  const exportCsvMobileButton = document.getElementById("export-csv-mobile");

  const appWindow = getCurrentWindow();

  document
    .querySelector('[data-tauri-drag-region]')
    ?.addEventListener('dragstart', (event) => {
      event.preventDefault();
    });

  let tasks = loadTasks();

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
        "flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 p-4 rounded-lg shadow-sm border border-gray-200 " +
        (task.isRunning ? "ring-2 ring-green-400" : "");

      const formattedTime = formatTime(task.elapsedTime);
      const buttonText = task.isRunning ? "Stop" : "Start";
      const buttonClass = task.isRunning
        ? "bg-red-500 hover:bg-red-600"
        : "bg-green-500 hover:bg-green-600";

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
            data-action="toggle"
            class="w-1/2 sm:w-20 p-2 rounded-lg text-white font-semibold transition duration-200 ${buttonClass}"
          >
            ${buttonText}
          </button>
          <button
            data-id="${task.id}"
            data-action="delete"
            class="w-1/2 sm:w-20 p-2 rounded-lg text-white font-semibold bg-gray-400 hover:bg-gray-500 transition duration-200"
          >
            Delete
          </button>
        </div>
      `;
      taskList.appendChild(taskElement);
    });
  }

  function toggleTimer(id) {
    const taskToToggle = tasks.find((task) => task.id === id);
    if (!taskToToggle) return;

    const isStarting = !taskToToggle.isRunning;

    if (isStarting) {
      tasks.forEach((task) => {
        if (task.isRunning) {
          clearInterval(task.intervalId);
          task.isRunning = false;
          task.intervalId = null;
        }
      });
    }

    if (isStarting) {
      taskToToggle.isRunning = true;
      taskToToggle.intervalId = setInterval(() => {
        taskToToggle.elapsedTime += 1;
        const timeElement = document.getElementById(`time-${taskToToggle.id}`);
        if (timeElement) {
          timeElement.textContent = formatTime(taskToToggle.elapsedTime);
        }
        saveTasks();
      }, 1000);
    } else {
      clearInterval(taskToToggle.intervalId);
      taskToToggle.isRunning = false;
      taskToToggle.intervalId = null;
    }

    saveTasks();
    renderTasks();
  }

  function deleteTask(id) {
    const taskToDelete = tasks.find((task) => task.id === id);
    if (taskToDelete && taskToDelete.isRunning) {
      clearInterval(taskToDelete.intervalId);
    }

    tasks = tasks.filter((task) => task.id !== id);
    saveTasks();
    renderTasks();
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
    };

    tasks.push(newTask);
    taskInput.value = "";
    saveTasks();
    renderTasks();
  }

  function saveTasks() {
    const tasksToSave = tasks.map((task) => ({
      id: task.id,
      label: task.label,
      elapsedTime: task.elapsedTime,
    }));
    localStorage.setItem("taskTimerApp", JSON.stringify(tasksToSave));
  }

  function loadTasks() {
    const savedTasks = localStorage.getItem("taskTimerApp");
    if (savedTasks) {
      return JSON.parse(savedTasks).map((task) => ({
        ...task,
        isRunning: false,
        intervalId: null,
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

  exportCsvButton?.addEventListener("click", exportTasksAsCsv);
  exportCsvMobileButton?.addEventListener("click", exportTasksAsCsv);

  taskList.addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (!button) return;

    const id = Number(button.dataset.id);
    const action = button.dataset.action;

    if (action === "toggle") {
      toggleTimer(id);
    } else if (action === "delete") {
      if (confirm("Are you sure you want to delete this task?")) {
        deleteTask(id);
      }
    }
  });

  renderTasks();
});
