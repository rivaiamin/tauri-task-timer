// Drives the real TUI in a pty (`script`) and measures the database it writes,
// so the picker and the automatic status sync are observed through the actual
// terminal UI rather than through the state machine in isolation.
//
//   node scripts/verify-status-picker.mjs             # G3
//   node scripts/verify-status-picker.mjs --self-test # control: the pre-change
//                                                     # behavior must fail here
//
// The pty is required: ratatui draws to a terminal, and a plain pipe gives it no
// window to draw into. Keys are written with a settle delay because the TUI
// polls for input on a 250ms tick.

import { spawn } from 'node:child_process';
import { DatabaseSync, } from 'node:sqlite';
import { mkdtempSync, mkdirSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');
const DATE = '2026-09-17';
const LABEL = 'US-905';

const IN_PROGRESS = '21';
const DONE = '31';

// Mirrors the deployed schema, including the label-uniqueness index the app
// relies on (apps/web/drizzle/0002_chubby_roulette.sql).
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
CREATE TABLE task_comments (id INTEGER PRIMARY KEY, task_id INTEGER, subject TEXT, summary TEXT, branch TEXT, pr TEXT);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY, task_id INTEGER, "group" TEXT, field TEXT, value TEXT);
`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Strip escape sequences so the drawn frame can be matched as text. */
function plain(text) {
  return text
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '\n')
    .replace(/\x1b[()][A-Z0-9]/g, '')
    .replace(/\x1b[<>=]/g, '')
    .replace(/\r/g, '\n');
}

function makeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'status-picker-'));
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });

  const email = `picker-${Date.now()}@example.test`;
  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)').run(
    email,
    email,
    'x'
  );
  const { lastInsertRowid } = db
    .prepare('INSERT INTO tasks (user_id, label, description) VALUES (?, ?, ?)')
    .run(email, LABEL, 'fixture ticket');
  db.prepare('INSERT INTO task_days (task_id, work_date) VALUES (?, ?)').run(lastInsertRowid, DATE);
  db.close();

  writeFileSync(
    join(configDir, 'config.toml'),
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') + '\n'
  );
  return { dir, dbPath, configDir };
}

function statusOf(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    const row = db.prepare('SELECT td.status, td.is_running, td.done FROM tasks t JOIN task_days td ON td.task_id = t.id WHERE t.label = ?').get(LABEL);
    return row;
  } finally {
    db.close();
  }
}

function reset(dbPath) {
  const db = new DatabaseSync(dbPath);
  db.prepare(
    "UPDATE task_days SET status='todo', is_running=0, done=0, start_time=NULL, elapsed_time=0 WHERE task_id = (SELECT id FROM tasks WHERE label=?)"
  ).run(LABEL);
  db.close();
}

/**
 * Run one TUI session in a pty, send `keys`, return the drawn frame as text.
 *
 */
async function session(fixture, keys) {
  const child = spawn(
    'script',
    [
      '-qec',
      `stty rows 45 cols 160; ${BIN} --config ${join(fixture.configDir, 'config.toml')} --date ${DATE}`,
      '/dev/null',
    ],
    { stdio: ['pipe', 'pipe', 'pipe'] }
  );
  let out = '';
  let err = '';
  child.stdout.on('data', (chunk) => {
    out += chunk.toString('utf8');
  });
  child.stderr.on('data', (chunk) => {
    err += chunk.toString('utf8');
  });

  const deadline = Date.now() + 12_000;
  while (!plain(out).includes(LABEL)) {
    if (Date.now() > deadline) {
      child.kill('SIGKILL');
      throw new Error('the TUI never drew its first frame in the pty');
    }
    await sleep(150);
  }

  for (const key of keys) {
    child.stdin.write(key);
    await sleep(700);
  }
  await sleep(400);
  const frame = plain(out);
  child.stdin.end();
  await sleep(200);
  child.kill('SIGKILL');
  return frame;
}

let failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

function assertToolchain() {
  if (!existsSync(BIN)) {
    failures.push(`${BIN} is missing — run: cargo build -p task-timer-tui`);
    return false;
  }
  if (process.platform !== 'linux') {
    failures.push('this check drives the TUI over a pty with `script`, which this platform lacks');
    return false;
  }
  return true;
}

/**
 * The picker oracle: the edit form's status field offers the catalog, and the
 * chosen row is what the database ends up holding. Then, on the same fixture,
 * a running task must show In Progress without any edit.
 */
async function pickerHolds(fixture) {
  reset(fixture.dbPath);
  check(statusOf(fixture.dbPath).status === 'todo', 'fixture must start at the default status');

  // e → open edit, Tab Tab → focus the status field, s → open the picker,
  // k×6 walks up from "No status" to row index 2, Enter applies, Enter saves.
  const frame = await session(fixture, ['e', '\t', '\t', 's', 'k', 'k', 'k', 'k', 'k', 'k', '\r', '\r']);
  check(
    !/already exists|constraint|not null|error/i.test(frame),
    `the save reported a failure:\n${frame.slice(-400)}`
  );

  const picked = statusOf(fixture.dbPath);
  check(
    picked.status === DONE,
    `the picked row must be stored, expected ${DONE} (Done), got '${picked.status}'`
  );

  // And the timer still drives the status on its own, with no edit involved.
  reset(fixture.dbPath);
  await session(fixture, [' ']);
  const running = statusOf(fixture.dbPath);
  check(
    runningTaskIsInProgress(running),
    `a running task must read In Progress (${IN_PROGRESS}), got '${running.status}' with is_running=${running.is_running}`
  );

  return failures.length === 0;
}

/**
 * The assertion the real check makes, as a pure predicate, so the control can
 * measure pre-change behavior with the *same* rule rather than a lookalike.
 */
function runningTaskIsInProgress(row) {
  return row.is_running === 1 && row.status === IN_PROGRESS;
}

/** Control: with the old "the timer never touches the status" behavior. */
async function pickerControl(fixture) {
  reset(fixture.dbPath);
  // Space starts the timer. Reproduce what the old binary left behind: a
  // running task whose status is still the default.
  await session(fixture, [' ']);
  if (statusOf(fixture.dbPath).is_running !== 1) {
    failures.push('the control fixture did not start its timer — inconclusive');
    return false;
  }
  const db = new DatabaseSync(fixture.dbPath);
  db.prepare("UPDATE task_days SET status='todo' WHERE task_id = (SELECT id FROM tasks WHERE label=?)").run(LABEL);
  db.close();

  const after = statusOf(fixture.dbPath);
  if (runningTaskIsInProgress(after)) {
    failures.push('the oracle accepted a running task whose status stayed behind — it cannot fail');
    return false;
  }
  return true;
}

async function runPicker() {
  const fixture = makeFixture();
  try {
    return await pickerHolds(fixture);
  } catch (error) {
    failures.push(`picker verification crashed: ${error.message}`);
    return false;
  } finally {
    rmSync(fixture.dir, { recursive: true, force: true });
  }
}

async function runSelfTest() {
  const fixture = makeFixture();
  try {
    const accepted = await pickerHolds(fixture);
    check(accepted, 'the oracle rejected the TUI it is supposed to verify');
    const rejected = await pickerControl(fixture);
    return accepted && rejected && failures.length === 0;
  } catch (error) {
    failures.push(`control crashed: ${error.message}`);
    return false;
  } finally {
    rmSync(fixture.dir, { recursive: true, force: true });
  }
}

let passed;
let marker;
if (process.argv.includes('--self-test')) {
  passed = assertToolchain() && (await runSelfTest());
  marker = 'status picker control passed';
} else {
  passed = assertToolchain() && (await runPicker());
  marker = 'status picker verification passed';
}

if (passed) {
  console.log(marker);
  process.exit(0);
}
for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
