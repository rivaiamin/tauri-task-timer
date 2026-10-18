// Verify kanban list against a throwaway DB.
import { spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "target", "debug", "task-timer-tui");
const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, label TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
CREATE TABLE task_days (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, work_date TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'todo', elapsed_time INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0, done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0, is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0, is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0, is_important INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, "group" TEXT NOT NULL, field TEXT NOT NULL, value TEXT, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
`;



function verifyDashboard() {
  const svelte = readFileSync(join(ROOT, "apps/web/src/routes/dashboard/kanban/+page.svelte"), "utf8");
  const server = readFileSync(join(ROOT, "apps/web/src/routes/dashboard/kanban/+page.server.ts"), "utf8");
  if (!svelte.includes("columnForJiraStatus") || !svelte.includes("/api/kanban/refresh")) {
    console.error("kanban dashboard page missing required hooks");
    process.exit(1);
  }
  if (!server.includes("listKanbanCards")) {
    console.error("kanban dashboard load missing listKanbanCards");
    process.exit(1);
  }
  console.log("kanban dashboard smoke passed");
}

function verifyParity() {
  const paths = [
    "apps/web/src/routes/api/kanban/cards/+server.ts",
    "apps/web/src/routes/dashboard/kanban/+page.svelte",
    "apps/web/src/lib/server/kanban.ts",
    "apps/web/src/lib/server/kanbanContracts.ts",
  ];
  for (const rel of paths) {
    if (!existsSync(join(ROOT, rel))) {
      console.error("missing", rel);
      process.exit(1);
    }
  }
  const mcp = readFileSync(join(ROOT, "apps/mcp/src/server.ts"), "utf8");
  for (const tool of ["kanban_list", "kanban_refresh", "kanban_pull"]) {
    if (!mcp.includes(`'${tool}'`)) {
      console.error("missing mcp tool", tool);
      process.exit(1);
    }
  }
  const webTest = spawnSync(
    "pnpm",
    ["exec", "vitest", "run", "src/lib/server/kanban.test.ts"],
    { cwd: join(ROOT, "apps/web"), encoding: "utf8" }
  );
  if (webTest.status !== 0) {
    console.error(webTest.stdout, webTest.stderr);
    process.exit(1);
  }
  if (!/Tests\s+1 passed/.test(webTest.stdout + webTest.stderr)) {
    console.error("web kanban test did not pass");
    process.exit(1);
  }
  console.log("kanban parity verification passed");
}

if (process.argv.includes("--dashboard")) {
  verifyDashboard();
  process.exit(0);
}

const parityOnly = process.argv.includes("--parity");
if (parityOnly) {
  verifyParity();
  process.exit(0);
}

const dir = mkdtempSync(join(tmpdir(), "kanban-verify-"));
const db = join(dir, "test.db");
const config = join(dir, "config.toml");
const { writeFileSync } = await import("node:fs");
writeFileSync(config, `database_path = "${db}"\nuser_email = "u@test"\n`);
const database = new DatabaseSync(db);
database.exec(SCHEMA);
database.exec("INSERT INTO users VALUES ('u1','u@test','x',0)");
database.exec("INSERT INTO tasks (user_id,label,created_at,updated_at) VALUES ('u1','US-1 fix',0,0)");
const tid = database.prepare("SELECT id FROM tasks").get().id;
database.exec(`INSERT INTO task_days (task_id,work_date,position,created_at,updated_at) VALUES (${tid},'2026-10-18',0,0,0)`);
database.exec(`INSERT INTO task_integrations (task_id,"group",field,value,created_at,updated_at) VALUES (${tid},'jira','issue_key','US-1',0,0)`);
database.exec(`INSERT INTO task_integrations (task_id,"group",field,value,created_at,updated_at) VALUES (${tid},'jira','status','To Do',0,0)`);
database.close();

const run = spawnSync(BIN, ["--config", config, "--date", "2026-10-18", "--json", "kanban", "list"], {
  encoding: "utf8",
});
if (run.status !== 0) {
  console.error(run.stdout, run.stderr);
  process.exit(1);
}
const rows = JSON.parse(run.stdout);
if (!Array.isArray(rows) || rows.length !== 1 || !(rows[0].issue_key === "US-1" || rows[0].issueKey === "US-1")) {
  console.error("unexpected kanban list", rows);
  process.exit(1);
}
rmSync(dir, { recursive: true, force: true });
console.log("kanban verification passed");
