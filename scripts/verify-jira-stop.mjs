// Verifies the TUI's JIRA stop behavior against a stub JIRA REST API — no
// credentials, no network.
//
//   node scripts/verify-jira-stop.mjs             # G1: stop logs work + returns to To Do
//   node scripts/verify-jira-stop.mjs --self-test # G2: the same oracle fails a worklog-only stop
//   node scripts/verify-jira-stop.mjs --paths     # G3: every stop path routes through the oracle
//
// The stub (scripts/jira-stub.mjs) runs as a separate process and appends every
// request it receives to a capture file, so the assertions read what actually
// went over the wire rather than what the source claims to send.

import { spawn, spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, mkdirSync, existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');
const STUB = join(ROOT, 'scripts', 'jira-stub.mjs');
const TODO = process.env.JIRA_STATUS_TODO || 'To Do';
const IN_PROGRESS = process.env.JIRA_STATUS_INPROGRESS || 'In Progress';
const DONE_STATUS = process.env.JIRA_STATUS_DONE || 'Cek di Local';

const mode = process.argv.includes('--self-test')
  ? 'self-test'
  : process.argv.includes('--paths')
    ? 'paths'
    : 'stop';

// ---------------------------------------------------------------------------
// Fixture — the TUI owns no schema, so the fixture carries what it reads.
// ---------------------------------------------------------------------------

const SCHEMA = `
CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, timer_mode TEXT DEFAULT 'focus');
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
CREATE TABLE task_comments (id INTEGER PRIMARY KEY, task_id INTEGER, subject TEXT, summary TEXT, branch TEXT, pr TEXT);
CREATE TABLE task_integrations (id INTEGER PRIMARY KEY, task_id INTEGER, "group" TEXT, field TEXT, value TEXT);
CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
`;

function makeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'jira-stop-'));
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });

  const email = `verify-${Date.now()}@example.test`;
  // The TUI/CLI resolve label lookups against the *local* date, while an ISO
  // timestamp is UTC — so pin the day and pass it explicitly. Using the UTC day
  // here made every command miss the fixture whenever local and UTC differ.
  const today = '2026-09-17';
  const key = 'US-901';

  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email) VALUES (?, ?)').run(email, email);
  db.prepare('INSERT INTO tasks (user_id, label, description, work_date) VALUES (?, ?, ?, ?)').run(
    email,
    key,
    'fixture ticket',
    today
  );
  db.close();

  writeFileSync(
    join(configDir, 'config.toml'),
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') + '\n'
  );
  return { dir, dbPath, configDir, key, today };
}

/** Read one scalar from the fixture, proving the binary moved local state too. */
function fixtureScalar(dbPath, sql) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    const row = db.prepare(sql).get();
    return row ? Object.values(row)[0] : undefined;
  } finally {
    db.close();
  }
}

function runCli(fixture, env, args) {
  return spawnSync(
    BIN,
    ['--config', join(fixture.configDir, 'config.toml'), '--date', fixture.today, ...args],
    {
      cwd: ROOT,
      encoding: 'utf8',
      timeout: 60_000,
      env: { ...process.env, ...env },
    }
  );
}

/** Requests the stub recorded so far, straight from the capture file. */
function captured(capturePath) {
  if (!existsSync(capturePath)) return [];
  return readFileSync(capturePath, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

/** The stub must be a separate process: spawnSync blocks this loop. */
async function startStub(staging) {
  const capturePath = join(staging, 'requests.jsonl');
  writeFileSync(capturePath, '');
  const child = spawn(process.execPath, [STUB, '--capture', capturePath], { stdio: ['ignore', 'pipe', 'inherit'] });

  const base = await new Promise((done, fail) => {
    let out = '';
    const timer = setTimeout(() => fail(new Error('stub did not report a port within 10s')), 10_000);
    child.stdout.on('data', (chunk) => {
      out += chunk;
      const match = out.match(/PORT (\d+)/);
      if (match) {
        clearTimeout(timer);
        done(`http://127.0.0.1:${match[1]}`);
      }
    });
    child.on('exit', (code) => {
      clearTimeout(timer);
      fail(new Error(`stub exited early with code ${code}`));
    });
  });

  return { base, capturePath, stop: () => child.kill('SIGTERM') };
}

function jiraEnv(fixture, base) {
  const envFile = join(fixture.dir, 'jira.env');
  writeFileSync(envFile, 'JIRA_EMAIL=verify@example.test\nJIRA_TOKEN=stub-token\n');
  return {
    JIRA_ENV_FILE: envFile,
    JIRA_SITE: base,
    JIRA_STATUS_TODO: TODO,
    JIRA_STATUS_INPROGRESS: IN_PROGRESS,
    JIRA_STATUS_DONE: DONE_STATUS,
  };
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

let failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

function hasWorklog(requests, key) {
  return requests.some(
    (r) => r.method === 'POST' && r.path === `/rest/api/3/issue/${key}/worklog` && (r.body?.timeSpentSeconds ?? 0) > 0
  );
}

// Mirrors the stub's workflow ids (scripts/jira-stub.mjs TRANSITIONS).
const STATUS_IDS = { 'To Do': '11', 'In Progress': '21', 'Cek di Local': '51' };

function transitionIdFor(statusName) {
  const match = Object.keys(STATUS_IDS).find((name) => name.toLowerCase() === statusName.toLowerCase());
  return match ? STATUS_IDS[match] : undefined;
}

function transitionsTo(requests, key, statusName) {
  const id = transitionIdFor(statusName);
  if (!id) return false;
  return requests.some(
    (r) => r.method === 'POST' && r.path === `/rest/api/3/issue/${key}/transitions` && r.body?.transition?.id === id
  );
}

/**
 * The stop oracle: measure a recorded request set for one task's stop.
 * `withTodoTransition` is the only difference between the real behavior and the
 * worklog-only control, so the control reuses this exact logic.
 */
function stopOracleHolds({ key, requests, withTodoTransition }) {
  failures = [];
  check(hasWorklog(requests, key), `${key}: no worklog POST carrying a positive timeSpentSeconds`);
  if (withTodoTransition) {
    check(
      transitionsTo(requests, key, TODO),
      `${key}: no transition POST to '${TODO}' — recorded transitions: ${JSON.stringify(
        requests.filter((r) => r.path.endsWith('/transitions')).map((r) => r.body)
      )}`
    );
    check(!transitionsTo(requests, key, DONE_STATUS), `${key}: a plain stop must not move the issue to '${DONE_STATUS}'`);
  }
  return failures.length === 0;
}

// ---------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------

function assertToolchain() {
  if (!existsSync(BIN)) {
    failures.push(`${BIN} is missing — run: cargo build -p task-timer-tui`);
    return false;
  }
  if (!existsSync(STUB)) {
    failures.push(`${STUB} is missing`);
    return false;
  }
  return true;
}

async function runStopVerification() {
  const fixture = makeFixture();
  let stub = null;
  let ok = false;
  try {
    stub = await startStub(fixture.dir);
    const env = jiraEnv(fixture, stub.base);
    const { key } = fixture;

    const started = runCli(fixture, env, ['start', key]);
    check(started.status === 0, `start exited ${started.status}: ${started.stderr}${started.stdout}`);
    check(transitionsTo(captured(stub.capturePath), key, IN_PROGRESS), `start did not move ${key} to '${IN_PROGRESS}'`);

    await new Promise((r) => setTimeout(r, 1_100)); // let the run accumulate a reportable second

    const stopped = runCli(fixture, env, ['stop', key]);
    check(stopped.status === 0, `stop exited ${stopped.status}: ${stopped.stderr}${stopped.stdout}`);

    const all = captured(stub.capturePath);
    check(
      fixtureScalar(fixture.dbPath, `SELECT done FROM tasks WHERE label='${key}'`) === 0,
      `${key}: fixture should not be done, so the stop path must return it to '${TODO}'`
    );

    ok = stopOracleHolds({ key, requests: all, withTodoTransition: true }) && failures.length === 0;
  } catch (error) {
    failures.push(`verification crashed: ${error.message}`);
  } finally {
    stub?.stop();
    rmSync(fixture.dir, { recursive: true, force: true });
  }
  return ok;
}

async function runSelfTest() {
  const fixture = makeFixture();
  let stub = null;
  let controlPassed = false;
  try {
    stub = await startStub(fixture.dir);
    const env = jiraEnv(fixture, stub.base);
    const { key } = fixture;

    runCli(fixture, env, ['start', key]);
    check(
      transitionsTo(captured(stub.capturePath), key, IN_PROGRESS),
      'control fixture never reached the stub — negative control inconclusive'
    );

    await new Promise((r) => setTimeout(r, 1_100));

    runCli(fixture, env, ['stop', key]);
    const observed = captured(stub.capturePath);
    check(observed.length > 0, 'control stop issued no JIRA requests');

    // Real stop traffic with the transitions stripped is the worklog-only
    // behavior this change replaces. The oracle must reject it.
    const worklogOnly = observed.filter((r) => !r.path.endsWith('/transitions'));
    const rejected = !stopOracleHolds({ key, requests: worklogOnly, withTodoTransition: true }) && failures.length > 0;
    check(rejected, 'the oracle accepted a worklog-only stop — it cannot fail, so it proves nothing');

    // And the same oracle must accept the real, untrimmed traffic.
    const accepted = stopOracleHolds({ key, requests: observed, withTodoTransition: true });
    check(accepted, "the oracle rejected the binary's real stop traffic");

    controlPassed = rejected && accepted && failures.length === 0;
  } catch (error) {
    failures.push(`control crashed: ${error.message}`);
  } finally {
    stub?.stop();
    rmSync(fixture.dir, { recursive: true, force: true });
  }
  return controlPassed;
}

function runPathCoverage() {
  failures = [];
  const read = (file) => readFileSync(join(ROOT, file), 'utf8');

  const stopPaths = [
    ['apps/tui/src/app/mod.rs', /fn stop_task[\s\S]*?\n    }\n/, 'TUI single stop'],
    ['apps/tui/src/app/mod.rs', /fn stop_all_running[\s\S]*?\n    }\n/, 'TUI hook pause / session end'],
    ['apps/tui/src/cli.rs', /Command::Stop \{ task \}[\s\S]*?Command::Reset/, 'CLI stop'],
  ];
  for (const [file, slice, label] of stopPaths) {
    const body = read(file).match(slice)?.[0] ?? '';
    check(body.length > 0, `${file}: could not locate the ${label} body — coverage check is blind`);
    check(/record_and_reopen/.test(body), `${file}: ${label} does not route through record_and_reopen`);
  }

  const jira = read('apps/tui/src/jira.rs');

  // Every stop path must reach the stop oracle, and the oracle must resolve an
  // unfinished run to the To Do status.
  const recordBody = jira.match(/pub fn record_and_reopen[\s\S]*?\n    }/)?.[0] ?? '';
  check(recordBody.length > 0, 'apps/tui/src/jira.rs: record_and_reopen not found');
  check(/fire_run_end\([\s\S]*?false,?\s*\)/.test(recordBody), 'record_and_reopen no longer requests the unfinished (false) run-end path');

  const stopBody = jira.match(/pub fn fire_on_stop[\s\S]*?\n}/)?.[0] ?? '';
  check(stopBody.length > 0, 'apps/tui/src/jira.rs: fire_on_stop not found');
  check(/fire_run_end\([\s\S]*?false,?\s*\)/.test(stopBody), 'fire_on_stop no longer requests the unfinished (false) run-end path');

  const doneBody = jira.match(/pub fn fire_on_done[\s\S]*?\n}/)?.[0] ?? '';
  check(doneBody.length > 0, 'apps/tui/src/jira.rs: fire_on_done not found');
  check(/fire_run_end\([\s\S]*?true,?\s*\)/.test(doneBody), 'fire_on_done no longer requests the done (true) run-end path');

  const plan = jira.match(/pub fn fire_run_end[\s\S]*?\n}/)?.[0] ?? '';
  check(plan.length > 0, 'apps/tui/src/jira.rs: fire_run_end not found');
  check(/status_when_run_ends\(task_is_done\)/.test(plan), 'fire_run_end no longer resolves the status from the done flag');

  const decide = jira.match(/pub fn status_when_run_ends[\s\S]*?\n}/)?.[0] ?? '';
  check(decide.length > 0, 'apps/tui/src/jira.rs: status_when_run_ends not found');
  check(
    /if task_is_done[\s\S]*?status_done\(\)[\s\S]*?else[\s\S]*?status_todo\(\)/.test(decide),
    'status_when_run_ends must map done→done status and unfinished→To Do'
  );

  const app = read('apps/tui/src/app/mod.rs');
  const toggle = app.match(/KeyCode::Char\('D'\)[\s\S]*?unmarked done/)?.[0] ?? '';
  check(toggle.length > 0, 'apps/tui/src/app/mod.rs: done toggle not found');
  check(/if new_done/.test(toggle), 'done toggle stopped gating the JIRA done hook on the done flag');

  return failures.length === 0;
}

// ---------------------------------------------------------------------------

let passed;
let marker;
if (mode === 'stop') {
  passed = assertToolchain() && (await runStopVerification());
  marker = 'stop verification passed';
} else if (mode === 'self-test') {
  passed = assertToolchain() && (await runSelfTest());
  marker = 'stop oracle control passed';
} else {
  passed = runPathCoverage();
  marker = 'stop path coverage passed';
}

if (passed) {
  console.log(marker);
  process.exit(0);
}
for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
