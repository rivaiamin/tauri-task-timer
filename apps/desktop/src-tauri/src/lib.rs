use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// Task structure matching the JavaScript version
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: i64,
    pub label: String,
    pub elapsed_time: i64, // in seconds
    pub position: i64, // for ordering
}

// Database connection wrapper
pub struct AppState {
    pub db: Mutex<Connection>,
}

// Initialize database and create table if it doesn't exist
fn init_db(app: &tauri::AppHandle) -> Result<Connection, Box<dyn std::error::Error>> {
    let app_data_dir = app.path()
        .app_data_dir()?;
    
    // Create parent directory if it doesn't exist
    std::fs::create_dir_all(&app_data_dir)?;
    
    let db_path = app_data_dir.join("task-timer.db");
    let conn = Connection::open(&db_path)?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            label TEXT NOT NULL,
            elapsed_time INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )?;
    
    // Add position column if it doesn't exist (for migration from old schema)
    let _ = conn.execute(
        "ALTER TABLE tasks ADD COLUMN position INTEGER NOT NULL DEFAULT 0",
        [],
    );
    
    Ok(conn)
}

#[tauri::command]
fn get_tasks(state: tauri::State<AppState>) -> Result<Vec<Task>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    let mut stmt = db
        .prepare("SELECT id, label, elapsed_time, position FROM tasks ORDER BY position, id")
        .map_err(|e| e.to_string())?;
    
    let task_iter = stmt
        .query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                label: row.get(1)?,
                elapsed_time: row.get(2)?,
                position: row.get(3).unwrap_or(0),
            })
        })
        .map_err(|e| e.to_string())?;
    
    let mut tasks = Vec::new();
    for task in task_iter {
        tasks.push(task.map_err(|e| e.to_string())?);
    }
    
    Ok(tasks)
}

#[tauri::command]
fn save_task(state: tauri::State<AppState>, task: Task) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    db.execute(
        "INSERT OR REPLACE INTO tasks (id, label, elapsed_time, position) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![task.id, task.label, task.elapsed_time, task.position],
    )
    .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn save_tasks(state: tauri::State<AppState>, tasks: Vec<Task>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    // Start a transaction for better performance
    let tx = db.transaction().map_err(|e| e.to_string())?;
    
    // Clear existing tasks
    tx.execute("DELETE FROM tasks", []).map_err(|e| e.to_string())?;
    
    // Insert all tasks with their positions
    for (index, task) in tasks.iter().enumerate() {
        tx.execute(
            "INSERT INTO tasks (id, label, elapsed_time, position) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![task.id, task.label, task.elapsed_time, index as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    
    tx.commit().map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn delete_task(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    db.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn reset_all_tasks(state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    db.execute("UPDATE tasks SET elapsed_time = 0", [])
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize database
            let conn = init_db(app.handle())
                .map_err(|e| format!("Failed to initialize database: {}", e))?;
            let app_state = AppState {
                db: Mutex::new(conn),
            };
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_tasks,
            save_task,
            save_tasks,
            delete_task,
            reset_all_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
