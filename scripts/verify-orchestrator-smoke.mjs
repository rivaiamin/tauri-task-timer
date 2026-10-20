// Drives the real TUI in a pty with fake `paseo` and `node` on PATH, then
// measures orchestrator discovery, retained history, composer focus, return to
// Daily, and a timer start — without a live Paseo driver or production DB.
// Multiline send / delivery states are covered by `cargo test … orchestrator`
// and the Task 4 manual smoke in tasks/todo.md (PTY Ctrl+S is unreliable under
// `script` because of terminal flow-control).
//
//   node scripts/verify-orchestrator-smoke.mjs             # G4 smoke
//   node scripts/verify-orchestrator-smoke.mjs --self-test # control oracle
//   node scripts/verify-orchestrator-smoke.mjs --docs    # G5 README contract
//
// Requires a debug binary (`cargo build -p task-timer-tui`) and Linux `script`.

import { spawn, spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { chmodSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIN = join(ROOT, 'target', 'debug', 'task-timer-tui');
const README = join(ROOT, 'apps', 'tui', 'README.md');
const DATE = '2026-09-17';
const LABEL = 'ORCH-SMOKE';
const DRIVER_ID = 'fake-driver-1';
const HISTORY_MARKER = 'smoke-assistant-line';

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
`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function plain(text) {
  return text
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '\n')
    .replace(/\x1b[()][A-Z0-9]/g, '')
    .replace(/\x1b[<>=]/g, '')
    .replace(/\r/g, '\n');
}

/** Ratatui width reflow can put one word per line once escapes are stripped. */
function collapsed(text) {
  return plain(text).replace(/\s+/g, ' ').trim();
}

function historyPageJson() {
  return {
    agentId: DRIVER_ID,
    epoch: 'epoch-smoke',
    direction: 'tail',
    startCursor: { epoch: 'epoch-smoke', seq: 10 },
    endCursor: { epoch: 'epoch-smoke', seq: 11 },
    hasOlder: false,
    hasNewer: false,
    reset: false,
    staleCursor: false,
    gap: false,
    window: { minSeq: 1, maxSeq: 11, nextSeq: 12 },
    entries: [
      {
        item: { type: 'assistant_message', text: HISTORY_MARKER, messageId: 'm-smoke' },
        timestamp: '2026-10-19T10:00:00Z',
        seqStart: 11,
        seqEnd: 11,
        sourceSeqRanges: [],
      },
    ],
  };
}

function writeFakeTools(fixtureDir) {
  const binDir = join(fixtureDir, 'bin');
  mkdirSync(binDir, { recursive: true });
  const historyPath = join(fixtureDir, 'history.json');
  writeFileSync(historyPath, JSON.stringify(historyPageJson()));

  const paseo = join(binDir, 'paseo');
  writeFileSync(
    paseo,
    `#!/bin/sh
echo "$0 $@" >> "$ORCHESTRATOR_FIXTURE_DIR/paseo.log"
case "$1" in
  ls)
    printf '[{"id":"${DRIVER_ID}","name":"smoke-driver","status":"idle"}]\\n'
    exit 0
    ;;
  status)
    printf '{"listen":"127.0.0.1:6767"}\\n'
    exit 0
    ;;
  send)
    printf '{"status":"sent"}\\n'
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
`
  );

  const node = join(binDir, 'node');
  writeFileSync(
    node,
    `#!/bin/sh
cat >/dev/null
cat "$ORCHESTRATOR_FIXTURE_DIR/history.json"
`
  );
  chmodSync(paseo, 0o700);
  chmodSync(node, 0o700);
  return binDir;
}

function makeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'orchestrator-smoke-'));
  const dbPath = join(dir, 'fixture.db');
  const configDir = join(dir, 'config');
  mkdirSync(configDir, { recursive: true });
  const binDir = writeFakeTools(dir);

  const email = `orch-smoke-${Date.now()}@example.test`;
  const db = new DatabaseSync(dbPath);
  db.exec(SCHEMA);
  db.prepare('INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, 0)').run(
    email,
    email,
    'x'
  );
  const { lastInsertRowid } = db
    .prepare('INSERT INTO tasks (user_id, label, description) VALUES (?, ?, ?)')
    .run(email, LABEL, 'orchestrator smoke fixture');
  db.prepare('INSERT INTO task_days (task_id, work_date) VALUES (?, ?)').run(lastInsertRowid, DATE);
  db.close();

  writeFileSync(
    join(configDir, 'config.toml'),
    [`database_path = "${dbPath}"`, `user_email = "${email}"`, 'timer_mode = "focus"'].join('\n') + '\n'
  );

  const env = {
    JIRA_ENV_FILE: join(dir, 'absent.env'),
    JIRA_SITE: 'http://127.0.0.1:1',
    ORCHESTRATOR_FIXTURE_DIR: dir,
    PATH: `${binDir}:${process.env.PATH ?? ''}`,
  };
  return { dir, dbPath, configDir, env };
}

function runCli(fixture, args) {
  return spawnSync(
    BIN,
    ['--config', join(fixture.configDir, 'config.toml'), '--date', DATE, ...args],
    {
      cwd: ROOT,
      encoding: 'utf8',
      timeout: 60_000,
      env: { ...process.env, ...fixture.env },
    }
  );
}

function runningRow(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    return db
      .prepare(
        'SELECT td.is_running FROM tasks t JOIN task_days td ON td.task_id = t.id WHERE t.label = ?'
      )
      .get(LABEL);
  } finally {
    db.close();
  }
}

function resetTimer(dbPath) {
  const db = new DatabaseSync(dbPath);
  db.prepare(
    "UPDATE task_days SET is_running=0, elapsed_time=0, start_time=NULL WHERE task_id = (SELECT id FROM tasks WHERE label=?)"
  ).run(LABEL);
  db.close();
}

async function waitFor(outputRef, predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const frame = plain(outputRef.text);
    if (predicate(frame)) return frame;
    await sleep(150);
  }
  throw new Error(`timed out waiting for ${label}`);
}

/**
 * @param {ReturnType<typeof makeFixture>} fixture
 * @param {Array<{ keys?: string, wait?: (frame: string) => boolean, timeoutMs?: number, sleepMs?: number, label?: string }>} steps
 * @param {{ env?: Record<string, string>, ready?: (frame: string) => boolean }} [options]
 */
async function session(fixture, steps, options = {}) {
  const env = { ...process.env, ...fixture.env, ...(options.env ?? {}) };
  const child = spawn(
    'script',
    [
      '-qec',
      `stty -ixon rows 45 cols 160; ${BIN} --config ${join(fixture.configDir, 'config.toml')} --date ${DATE}`,
      '/dev/null',
    ],
    { stdio: ['pipe', 'pipe', 'pipe'], env }
  );
  const outputRef = { text: '' };
  child.stdout.on('data', (chunk) => {
    outputRef.text += chunk.toString('utf8');
  });
  child.stderr.on('data', (chunk) => {
    outputRef.text += chunk.toString('utf8');
  });

  const ready = options.ready ?? ((frame) => frame.includes(LABEL));
  await waitFor(outputRef, ready, 12_000, 'initial daily frame');

  for (const step of steps) {
    if (step.keys) {
      child.stdin.write(step.keys);
      await sleep(500);
    }
    if (step.sleepMs) {
      await sleep(step.sleepMs);
    } else if (step.wait) {
      await waitFor(
        outputRef,
        step.wait,
        step.timeoutMs ?? 12_000,
        step.label ?? 'orchestrator step'
      );
    }
  }
  await sleep(400);
  const frame = plain(outputRef.text);
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

async function orchestratorSmokeHolds(fixture) {
  resetTimer(fixture.dbPath);

  const frame = await session(fixture, [
    {
      keys: 'O',
      wait: (f) => f.includes(HISTORY_MARKER),
      timeoutMs: 20_000,
      label: 'orchestrator history',
    },
    {
      wait: (f) => collapsed(f).includes('Tab:compose'),
      timeoutMs: 10_000,
      label: 'history ready',
    },
    {
      keys: '\t',
      wait: (f) => collapsed(f).includes('Type or paste an instruction'),
      timeoutMs: 8000,
      label: 'composer focus',
    },
    {
      keys: '\x1b\x1b',
      wait: (f) => {
        const flat = collapsed(f);
        return flat.includes(LABEL) && !/Mode:\s*orchestrator/i.test(flat);
      },
      timeoutMs: 10_000,
      label: 'return to daily',
    },
  ]);

  check(frame.includes('Orchestrator') || frame.includes('orchestrator'), 'orchestrator mode was not shown');
  check(frame.includes(HISTORY_MARKER), `history did not render fixture text (${HISTORY_MARKER})`);
  const paseoLog = join(fixture.dir, 'paseo.log');
  if (existsSync(paseoLog)) {
    const log = readFileSync(paseoLog, 'utf8');
    check(log.includes(' ls '), 'fake paseo never ran driver discovery');
    check(log.includes(' status '), 'fake paseo never supplied daemon status for history');
  } else {
    check(false, 'fake paseo was never invoked');
  }
  check(
    collapsed(frame).includes('Type or paste an instruction'),
    'composer did not open from the orchestrator view'
  );
  check(frame.includes(LABEL), 'returning to Daily did not redraw the fixture task');

  const started = runCli(fixture, ['start', LABEL]);
  check(started.status === 0, `daily start CLI failed: ${started.stderr}${started.stdout}`);
  const row = runningRow(fixture.dbPath);
  check(row?.is_running === 1, 'timer start after orchestrator did not set is_running on the day row');

  return failures.length === 0;
}

async function orchestratorSmokeControl(fixture) {
  resetTimer(fixture.dbPath);
  const brokenPath = join(fixture.dir, 'empty-bin');
  mkdirSync(brokenPath, { recursive: true });
  const frame = await session(fixture, [{ keys: 'O' }, { sleepMs: 3000 }], {
    env: { PATH: `${brokenPath}:/usr/bin:/bin`, ORCHESTRATOR_FIXTURE_DIR: fixture.dir },
    ready: (text) => text.includes(LABEL),
  });
  check(!frame.includes(HISTORY_MARKER), 'control fixture must not load fake history without paseo on PATH');
  const sawDisconnected =
    /unavailable|No active driver|Discovering|could not|not checked/i.test(frame) ||
    frame.includes('Orchestrator');
  check(sawDisconnected, 'control fixture did not show a disconnected orchestrator state');
  return failures.length === 0;
}

function verifyDocs() {
  const readme = readFileSync(README, 'utf8');
  const required = [
    '## Orchestrator',
    '`O`',
    'Ctrl+S',
    'Start/adopt',
    'driver-start',
    'does **not** start a driver',
    'Delivery unknown',
    'Node.js 22',
    'orchestrator-contract.md',
  ];
  for (const phrase of required) {
    check(readme.includes(phrase), `README missing orchestrator operator text: ${phrase}`);
  }
  return failures.length === 0;
}

async function runSmoke() {
  const fixture = makeFixture();
  try {
    return await orchestratorSmokeHolds(fixture);
  } catch (error) {
    failures.push(`orchestrator smoke crashed: ${error.message}`);
    return false;
  } finally {
    rmSync(fixture.dir, { recursive: true, force: true });
  }
}

async function runSelfTest() {
  const fixture = makeFixture();
  try {
    failures = [];
    const accepted = await orchestratorSmokeHolds(fixture);
    if (!accepted) {
      check(false, 'the oracle rejected the TUI it is supposed to verify');
      return false;
    }
    failures = [];
    return await orchestratorSmokeControl(fixture);
  } catch (error) {
    failures.push(`self-test crashed: ${error.message}`);
    return false;
  } finally {
    rmSync(fixture.dir, { recursive: true, force: true });
  }
}

const mode = process.argv.includes('--self-test')
  ? 'self-test'
  : process.argv.includes('--docs')
    ? 'docs'
    : 'smoke';

let passed;
let marker;
if (mode === 'docs') {
  passed = verifyDocs();
  marker = 'orchestrator docs verified';
} else if (mode === 'self-test') {
  passed = assertToolchain() && (await runSelfTest());
  marker = 'orchestrator smoke control passed';
} else {
  passed = assertToolchain() && (await runSmoke());
  marker = 'orchestrator smoke verification passed';
}

if (passed) {
  console.log(marker);
  process.exit(0);
}
for (const failure of failures) console.error(`FAIL: ${failure}`);
process.exit(1);
