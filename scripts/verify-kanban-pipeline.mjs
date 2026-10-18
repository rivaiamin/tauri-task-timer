import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "target", "debug", "task-timer-tui");

const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
`;

const dir = mkdtempSync(join(tmpdir(), "kanban-pipe-"));
const dbPath = join(dir, "fixture.db");
const configDir = join(dir, "config");
const binDir = join(dir, "bin");
mkdirSync(configDir, { recursive: true });
mkdirSync(binDir, { recursive: true });

const stubBin = join(binDir, "jira-local-ok-pr-sync");
writeFileSync(stubBin, "#!/bin/sh\necho '{\"stub\":true}'\nexit 0\n");
chmodSync(stubBin, 0o755);

const email = "pipe@test";
const db = new DatabaseSync(dbPath);
db.exec(SCHEMA);
db.prepare("INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)").run(
  "u1",
  email,
  "x"
);
db.close();

const configPath = join(configDir, "config.toml");
writeFileSync(
  configPath,
  [
    `database_path = "${dbPath}"`,
    `user_email = "${email}"`,
    'jira_monitor_projects = ["US"]',
    "[jira_pipelines]",
    `local_ok_pr_sync = "${stubBin}"`,
  ].join("\n") + "\n"
);

const run = spawnSync(
  BIN,
  ["--config", configPath, "--json", "kanban", "pipeline", "local-ok", "--dry-run"],
  { encoding: "utf8", env: { ...process.env, PATH: `${binDir}:${process.env.PATH ?? ""}` } }
);

if (run.status !== 0) {
  console.error(run.stdout, run.stderr);
  rmSync(dir, { recursive: true, force: true });
  process.exit(1);
}

rmSync(dir, { recursive: true, force: true });
console.log("kanban pipeline stub verification passed");
