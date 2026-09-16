// Verifies that a task's stored status follows its run state, without JIRA,
// credentials, or network.
//
//   node scripts/verify-status-sync.mjs --cli       # G1: start → In Progress, done → Done
//   node scripts/verify-status-sync.mjs --self-test # G2: pre-change behavior fails the oracle
//
// The fixture has no JIRA credentials on purpose: the status shown in the list
// must not depend on JIRA being reachable. `--self-test` reproduces the
// pre-change behavior (the timer moves, the status stays) and proves this
// oracle rejects it, so a green run means something.

import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, mkdirSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');

const mode = process.argv.includes('--self-test') ? 'self-test' : 'cli';

// The catalog ids are the app's, from apps/tui/src/jira.rs JIRA_STATUSES.
const IN_PROGRESS = '21';
const DONE = '31';

const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id TEXT NOT NULL, label TEXT NOT NULL, description TEXT, code TEXT, link TEXT,
  status TEXT NOT NULL DEFAULT 'todo', notes TEXT, tags TEXT,
  elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0,
  is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0,
  is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER,
  work_date TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
`;

function makeFixture(label) {
  const dir = mkdtempSync(join(tmpdir(), 'status-sync-'));
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });

  const email = `status-${Date.now()}@example.test`;
  // Pinned, and passed to every CLI call: the TUI's default is the local date,
  // which is not the UTC date this fixture would otherwise pick.
  const today = '2026-09-17';

  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)').run(
    email,
    email,
    'x'
  );
  db.prepare('INSERT INTO tasks (user_id, label, work_date) VALUES (?, ?, ?)').run(email, label, today);
  db.close();

  writeFileSync(
    join(configDir, 'config.toml'),
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') + '\n'
  );
  // No JIRA credentials on purpose: the status shown in the list must not depend
  // on JIRA being reachable.
  const env = { JIRA_ENV_FILE: join(dir, 'absent.env'), JIRA_SITE: 'http://127.0.0.1:1' };
  return { dir, dbPath, configDir, email, label, date: today, env };
}

function runCli(fixture, args) {
  return spawnSync(
    BIN,
    ['--config', join(fixture.configDir, 'config.toml'), '--date', fixture.date, ...args],
    {
      cwd: ROOT,
      encoding: 'utf8',
      timeout: 60_000,
      env: { ...process.env, ...fixture.env },
    }
  );
}

/** Read the row back from the DB — the list view reads exactly this. */
function row(dbPath, label) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    return db.prepare('SELECT status, is_running, done FROM tasks WHERE label = ?').get(label);
  } finally {
    db.close();
  }
}

let failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

/**
 * Run `fn` with its own diagnostic sink, returning whether it found nothing to
 * complain about. The control deliberately provokes complaints; keeping them out
 * of the top-level `failures` is what lets a rejected control still be a pass.
 */
function isolated(fn) {
  const outer = failures;
  failures = [];
  try {
    const result = fn();
    const clean = failures.length === 0 ? '' : failures.join('; ');
    failures = outer;
    return { clean, ok: result === true && clean === '' };
  } catch (error) {
    const caught = `${error.message}`;
    failures = outer;
    return { clean: caught, ok: false };
  }
}

function assertToolchain() {
  if (!existsSync(BIN)) {
    failures.push(`${BIN} is missing — run: cargo build -p task-timer-tui`);
    return false;
  }
  return true;
}

/**
 * The status-sync oracle. `preChange` reproduces the old behavior — timer moves,
 * status is left where it was — so the control drives the same assertions.
 */
function statusSyncHolds(fixture, { preChange }) {
  const { dbPath, label } = fixture;
  const before = row(dbPath, label);
  check(before !== undefined, `${label}: fixture row missing`);
  if (before === undefined) return false;

  const afterCreate = before.status;
  check(afterCreate !== IN_PROGRESS, `${label}: fixture must start outside In Progress (${IN_PROGRESS})`);

  const started = runCli(fixture, ['start', label]);
  check(started.status === 0, `start exited ${started.status}: ${started.stderr}${started.stdout}`);

  const running = row(dbPath, label);
  check(running.is_running === 1, `${label}: start did not set is_running`);

  // The pre-change binary updated no status at all; simulate it by restoring the
  // status the row had before the start, then measure that against the oracle.
  if (preChange) {
    const db = new DatabaseSync(dbPath);
    db.prepare('UPDATE tasks SET status = ? WHERE label = ?').run(afterCreate, label);
    db.close();
  }

  const whileRunning = row(dbPath, label);
  check(
    whileRunning.status === IN_PROGRESS,
    `${label}: a running task must read In Progress (${IN_PROGRESS}), got '${whileRunning.status}'`
  );

  const stopped = runCli(fixture, ['stop', label]);
  check(stopped.status === 0, `stop exited ${stopped.status}: ${stopped.stderr}${stopped.stdout}`);

  const stoppedRow = row(dbPath, label);
  check(stoppedRow.is_running === 0, `${label}: stop did not clear is_running`);
  check(
    stoppedRow.status === IN_PROGRESS,
    `${label}: a plain stop must leave the status alone, got '${stoppedRow.status}'`
  );

  const done = runCli(fixture, ['done', label]);
  check(done.status === 0, `done exited ${done.status}: ${done.stderr}${done.stdout}`);

  const doneRow = row(dbPath, label);
  check(doneRow.done === 1, `${label}: done did not set the done flag`);
  check(
    doneRow.status === DONE,
    `${label}: a done task must read Done (${DONE}), got '${doneRow.status}'`
  );

  return failures.length === 0;
}

async function runCliCheck() {
  const fixture = makeFixture('US-902');
  try {
    const result = isolated(() => statusSyncHolds(fixture, { preChange: false }));
    if (!result.ok) failures.push(`the oracle rejected the binary it verifies: ${result.clean}`);
  } catch (error) {
    failures.push(`verification crashed: ${error.message}`);
  } finally {
    rmSync(fixture.dir, { recursive: true, force: true });
  }
  return failures.length === 0;
}

async function runSelfTest() {
  const real = makeFixture('US-903');
  const control = makeFixture('US-904');
  try {
    // Same assertion, real behavior: must hold.
    const accepted = isolated(() => statusSyncHolds(real, { preChange: false }));
    check(accepted.ok, `the oracle rejected the binary it is supposed to verify: ${accepted.clean}`);

    // Same assertion, pre-change behavior: must be rejected, with a reason.
    const rejected = isolated(() => statusSyncHolds(control, { preChange: true }));
    check(!rejected.ok, 'the oracle accepted a timer move that left the status behind — it cannot fail');
    check(
      rejected.clean.length > 0,
      'the control was rejected without a diagnostic, so nothing shows what it caught'
    );

    return failures.length === 0;
  } catch (error) {
    failures.push(`control crashed: ${error.message}`);
    return false;
  } finally {
    rmSync(real.dir, { recursive: true, force: true });
    rmSync(control.dir, { recursive: true, force: true });
  }
}

let passed;
let marker;
if (mode === 'cli') {
  passed = assertToolchain() && (await runCliCheck());
  marker = 'status sync verification passed';
} else {
  passed = assertToolchain() && (await runSelfTest());
  marker = 'status sync control passed';
}

if (passed) {
  console.log(marker);
  process.exit(0);
}
for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
