// Stub-JIRA verification for kanban pull and refresh (no Atlassian network).
//
//   node scripts/verify-kanban-jira.mjs --pull
//   node scripts/verify-kanban-jira.mjs --refresh
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { DatabaseSync } from "node:sqlite";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "target", "debug", "task-timer-tui");
const DATE = "2026-10-18";
const PULL_KEY = "US-200";
const REFRESH_KEY = "US-1";

const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id TEXT NOT NULL, label TEXT NOT NULL, code TEXT, description TEXT, link TEXT,
  notes TEXT, tags TEXT,
  created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
CREATE TABLE task_days (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id INTEGER NOT NULL, work_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'todo',
  elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0,
  is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0,
  is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER,
  created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
CREATE TABLE task_comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, subject TEXT, summary TEXT, branch TEXT, pr TEXT, created_at INTEGER NOT NULL);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, "group" TEXT NOT NULL, field TEXT NOT NULL, value TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
`;

function jiraEnv(site, configDir) {
  return {
    ...process.env,
    JIRA_SITE: site,
    JIRA_EMAIL: "stub@example.test",
    JIRA_TOKEN: "stub-token",
    JIRA_ENV_FILE: join(configDir, "absent.env"),
  };
}

function startStub() {
  const searchHits = [];
  const issueHits = [];
  const server = createServer((req, res) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks).toString("utf8");
      const url = req.url ?? "";
      if (req.method === "POST" && url.includes("/search")) {
        searchHits.push(body);
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            issues: [
              {
                key: PULL_KEY,
                fields: { summary: "Kanban pull stub", status: { name: "To Do" } },
              },
            ],
          })
        );
        return;
      }
      if (req.method === "GET" && url.startsWith(`/rest/api/3/issue/${REFRESH_KEY}`)) {
        issueHits.push(url);
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            fields: { summary: "Refresh stub", status: { name: "In Progress" } },
          })
        );
        return;
      }
      if (req.method === "GET" && url.startsWith(`/rest/api/3/issue/${PULL_KEY}`)) {
        issueHits.push(url);
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            fields: { summary: "Kanban pull stub", status: { name: "To Do" } },
          })
        );
        return;
      }
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ errorMessages: ["not found"] }));
    });
  });
  return new Promise((ok) => {
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      ok({
        url: `http://127.0.0.1:${port}`,
        searchHits,
        issueHits,
        close: () => new Promise((done) => server.close(done)),
      });
    });
  });
}

function makeFixture(seedRefresh = false) {
  const dir = mkdtempSync(join(tmpdir(), "kanban-jira-"));
  const dbPath = join(dir, "fixture.db");
  const configDir = join(dir, "config");
  mkdirSync(configDir, { recursive: true });
  const email = "kanban-stub@test";
  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare("INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)").run(
    "u1",
    email,
    "x"
  );
  if (seedRefresh) {
    db.exec(
      `INSERT INTO tasks (user_id, label, created_at, updated_at) VALUES ('u1', 'US-1 fix', 0, 0)`
    );
    const tid = db.prepare("SELECT id FROM tasks").get().id;
    db.exec(
      `INSERT INTO task_days (task_id, work_date, position, created_at, updated_at) VALUES (${tid}, '${DATE}', 0, 0, 0)`
    );
    db.exec(
      `INSERT INTO task_integrations (task_id, "group", field, value, created_at, updated_at) VALUES (${tid}, 'jira', 'issue_key', '${REFRESH_KEY}', 0, 0)`
    );
    db.exec(
      `INSERT INTO task_integrations (task_id, "group", field, value, created_at, updated_at) VALUES (${tid}, 'jira', 'status', 'To Do', 0, 0)`
    );
  }
  db.close();
  const configPath = join(configDir, "config.toml");
  writeFileSync(
    configPath,
    [
      `database_path = "${dbPath}"`,
      `user_email = "${email}"`,
      'jira_monitor_projects = ["US"]',
    ].join("\n") + "\n"
  );
  return { dir, dbPath, configPath, configDir };
}

function runKanban(fixture, site, subcmd) {
  const args = ["--config", fixture.configPath, "--date", DATE, "--json", "kanban", subcmd];
  return new Promise((done) => {
    const child = spawn(BIN, args, {
      timeout: 60_000,
      env: jiraEnv(site, fixture.configDir),
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (c) => {
      stdout += c.toString("utf8");
    });
    child.stderr.on("data", (c) => {
      stderr += c.toString("utf8");
    });
    child.on("close", (status) => done({ status, stdout, stderr }));
  });
}

function jiraStatus(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  const row = db
    .prepare(
      `SELECT value FROM task_integrations WHERE "group" = 'jira' AND field = 'status' ORDER BY id DESC LIMIT 1`
    )
    .get();
  db.close();
  return row?.value ?? null;
}

function issueKeyCount(dbPath, key) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  const row = db
    .prepare(
      `SELECT COUNT(*) AS c FROM task_integrations WHERE "group" = 'jira' AND field = 'issue_key' AND value = ?`
    )
    .get(key);
  db.close();
  return row?.c ?? 0;
}

async function verifyPull() {
  const stub = await startStub();
  const fixture = makeFixture(false);
  try {
    const run = await runKanban(fixture, stub.url, "pull");
    if (run.status !== 0) {
      console.error(run.stdout, run.stderr);
      process.exit(1);
    }
    if (stub.searchHits.length < 1) {
      console.error("expected JQL search request");
      process.exit(1);
    }
    const body = JSON.parse(run.stdout);
    if (body.ingested !== 1) {
      console.error("unexpected pull report", body);
      process.exit(1);
    }
    if (issueKeyCount(fixture.dbPath, PULL_KEY) !== 1) {
      console.error("issue_key not written for pull");
      process.exit(1);
    }
    const dayRows = new DatabaseSync(fixture.dbPath, { readOnly: true })
      .prepare("SELECT COUNT(*) AS c FROM task_days")
      .get()?.c;
    if (dayRows !== 0) {
      console.error("kanban pull must not create task_days rows", dayRows);
      process.exit(1);
    }
  } finally {
    await stub.close();
    rmSync(fixture.dir, { recursive: true, force: true });
  }
  console.log("kanban pull stub verification passed");
}

async function verifyRefresh() {
  const stub = await startStub();
  const fixture = makeFixture(true);
  try {
    const run = await runKanban(fixture, stub.url, "refresh");
    if (run.status !== 0) {
      console.error(run.stdout, run.stderr);
      process.exit(1);
    }
    if (stub.issueHits.length < 1) {
      console.error("expected issue fetch on refresh");
      process.exit(1);
    }
    const status = jiraStatus(fixture.dbPath);
    if (status !== "In Progress") {
      console.error("status not updated", status);
      process.exit(1);
    }
  } finally {
    await stub.close();
    rmSync(fixture.dir, { recursive: true, force: true });
  }
  console.log("kanban refresh stub verification passed");
}

if (process.argv.includes("--refresh")) await verifyRefresh();
else await verifyPull();
