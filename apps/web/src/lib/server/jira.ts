// JIRA sync for the timer. Reads a local API token (~/.aimsis/jira.env) and drives
// AIMSIS JIRA Cloud (REST v3) off timer events, so the user never opens the JIRA UI:
//   start -> In Progress   |   focus-switch -> previous back to To Do + worklog
//   stop  -> worklog       |   "done" checkbox -> Cek lokal
//
// Fully disabled (every hook a no-op) when no creds are found, so the OSS app and
// other users are unaffected. Hooks are fire-and-forget: a JIRA failure is logged
// and dropped — the timer must never block or fail because JIRA is slow/down.
// ponytail: fire-and-forget, no retry/outbox. Add one only if drops become a problem.
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const STATUS_TODO = process.env.JIRA_STATUS_TODO || 'To Do';
const STATUS_INPROGRESS = process.env.JIRA_STATUS_INPROGRESS || 'In Progress';
const STATUS_DONE = process.env.JIRA_STATUS_DONE || 'Cek lokal';

interface Creds {
  email: string;
  token: string;
}
interface Linkable {
  label: string;
  description: string | null;
}
interface Transition {
  id: string;
  to?: { name?: string };
}

let credsCache: Creds | null | undefined;

function site(): string {
  return (process.env.JIRA_SITE || 'https://aimsis.atlassian.net').replace(/\/$/, '');
}

/** Read email+token from process.env, falling back to the ~/.aimsis/jira.env file.
 *  Cached; returns null (→ every hook no-ops) when either value is missing. */
function loadCreds(): Creds | null {
  if (credsCache !== undefined) return credsCache;
  let email = process.env.JIRA_EMAIL;
  let token = process.env.JIRA_TOKEN;
  if (!email || !token) {
    const file = process.env.JIRA_ENV_FILE || join(homedir(), '.aimsis', 'jira.env');
    try {
      for (const line of readFileSync(file, 'utf8').split('\n')) {
        const m = line.match(/^\s*(?:export\s+)?(JIRA_EMAIL|JIRA_TOKEN)\s*=\s*["']?([^"'\n]*)["']?/);
        if (!m) continue;
        if (m[1] === 'JIRA_EMAIL') email ||= m[2];
        else token ||= m[2];
      }
    } catch {
      /* no file → unconfigured */
    }
  }
  credsCache = email && token ? { email, token } : null;
  return credsCache;
}

/** First JIRA key (e.g. US-1459) found in the label, then the description. */
export function parseIssueKey(label: string, description: string | null): string | null {
  const re = /\b[A-Z][A-Z0-9]+-\d+\b/;
  return label.match(re)?.[0] ?? description?.match(re)?.[0] ?? null;
}

/** Pick the transition whose target status matches (case-insensitive), else null. */
export function pickTransitionId(transitions: Transition[], statusName: string): string | null {
  const want = statusName.toLowerCase();
  return transitions.find((t) => t.to?.name?.toLowerCase() === want)?.id ?? null;
}

function jiraTimestamp(ms: number): string {
  // JIRA wants yyyy-MM-ddTHH:mm:ss.SSS+0000 (no colon in the offset).
  return new Date(ms).toISOString().replace('Z', '+0000');
}

async function jiraFetch(path: string, method = 'GET', body?: unknown): Promise<any> {
  const creds = loadCreds();
  if (!creds) throw new Error('jira: unconfigured');
  const auth = Buffer.from(`${creds.email}:${creds.token}`).toString('base64');
  const res = await fetch(`${site()}/rest/api/3${path}`, {
    method,
    headers: {
      authorization: `Basic ${auth}`,
      accept: 'application/json',
      ...(body !== undefined ? { 'content-type': 'application/json' } : {})
    },
    body: body !== undefined ? JSON.stringify(body) : undefined
  });
  if (!res.ok) throw new Error(`jira ${method} ${path} -> ${res.status} ${await res.text()}`);
  const ct = res.headers.get('content-type') ?? '';
  return ct.includes('application/json') ? res.json() : null;
}

async function transitionTo(key: string, statusName: string): Promise<void> {
  const data = await jiraFetch(`/issue/${key}/transitions`);
  const id = pickTransitionId(data?.transitions ?? [], statusName);
  if (id) await jiraFetch(`/issue/${key}/transitions`, 'POST', { transition: { id } });
  // else: already in that status / no such transition — nothing to do.
}

async function logWork(key: string, seconds: number, startedMs: number | null): Promise<void> {
  if (!seconds || seconds < 60) return; // JIRA rejects worklogs under a minute.
  const started = jiraTimestamp(startedMs ?? Date.now() - seconds * 1000);
  await jiraFetch(`/issue/${key}/worklog`, 'POST', { timeSpentSeconds: Math.round(seconds), started });
}

async function run(task: Linkable, fn: (key: string) => Promise<void>): Promise<void> {
  if (!loadCreds()) return;
  const key = parseIssueKey(task.label, task.description);
  if (!key) return;
  try {
    await fn(key);
  } catch (err) {
    console.error('[jira] sync failed (dropped):', err instanceof Error ? err.message : err);
  }
}

/** Timer started → move the issue to In Progress. */
export const onStart = (task: Linkable): Promise<void> =>
  run(task, (key) => transitionTo(key, STATUS_INPROGRESS));

/** Focus-switch demoted this task → log the run, move it back to To Do. */
export const onSwitchStop = (task: Linkable, seconds: number, startedMs: number | null): Promise<void> =>
  run(task, async (key) => {
    await logWork(key, seconds, startedMs);
    await transitionTo(key, STATUS_TODO);
  });

/** Explicit stop → log the run, keep the current status. */
export const onStop = (task: Linkable, seconds: number, startedMs: number | null): Promise<void> =>
  run(task, (key) => logWork(key, seconds, startedMs));

/** "Done" checkbox → log any final run, move the issue to Cek lokal. */
export const onDone = (task: Linkable, seconds = 0, startedMs: number | null = null): Promise<void> =>
  run(task, async (key) => {
    await logWork(key, seconds, startedMs);
    await transitionTo(key, STATUS_DONE);
  });

// Self-check for the two pure bits (the non-trivial logic):
//   node --experimental-strip-types src/lib/server/jira.ts --selftest
if (process.argv.includes('--selftest')) {
  const eq = (got: unknown, want: unknown, msg: string) => {
    if (got !== want) throw new Error(`${msg}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  eq(parseIssueKey('US-1459 fix bug', null), 'US-1459', 'key from label');
  eq(parseIssueKey('fix bug', 'see AIM-22 for detail'), 'AIM-22', 'key from description');
  eq(parseIssueKey('no key here', 'still none'), null, 'no key');
  const tr: Transition[] = [
    { id: '11', to: { name: 'To Do' } },
    { id: '21', to: { name: 'In Progress' } }
  ];
  eq(pickTransitionId(tr, 'in progress'), '21', 'case-insensitive match');
  eq(pickTransitionId(tr, 'To Do'), '11', 'exact match');
  eq(pickTransitionId(tr, 'Cek lokal'), null, 'no matching transition');
  console.log('jira.ts self-check OK');
}
