# Task Timer

A desktop application for tracking time spent on multiple tasks. Built with Tauri, vanilla HTML, CSS, and JavaScript.

## Features

- **Multi-Task Management**: Add and manage multiple tasks simultaneously
- **Time Tracking**: Track elapsed time for each task with a visual timer display
- **Single Active Timer**: Only one timer can run at a time to ensure focused tracking
- **Persistent Storage**: Tasks and their elapsed times are saved in localStorage
- **CSV Export**: Export all tasks with calculated story points (1 story point = 1 hour) to CSV format
- **Responsive Design**: Works seamlessly on both desktop and mobile devices
- **Modern UI**: Clean, intuitive interface built with Tailwind CSS

## How It Works

1. **Add Tasks**: Enter a task name and click "Add Task" to create a new timer
2. **Start/Stop Timer**: Click "Start" to begin tracking time for a task. Only one timer can be active at a time
3. **View Progress**: See elapsed time displayed in HH:MM:SS format for each task
4. **Delete Tasks**: Remove tasks you no longer need
5. **Export Data**: Export all tasks with their story points to a CSV file for reporting or analysis

## Development

### Prerequisites

- Node.js
- Rust (for Tauri)
- System dependencies for Tauri (see [Tauri documentation](https://tauri.app/v1/guides/getting-started/prerequisites))

### Setup

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run build
npm run tauri build
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
