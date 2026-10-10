// Verifies the web app moves a task's own `status` on every timer layout
// change, matching the TUI's `auto_status`, and that the same oracle rejects
// the pre-fix behavior instead of passing on anything.
//
//   node scripts/verify-web-status.mjs             # web status verification passed
//   node scripts/verify-web-status.mjs --self-test # web status control passed
//
// The two status specs are the oracle:
//   - taskStatus.test.ts          the pure rules (a port of the TUI's
//                                 `auto_status_follows_the_rows_own_state`)
//   - taskService.status.test.ts  the wiring in start/stop/reset/done
//
// The control mutates the source so a stop leaves In Progress — the exact bug
// this work removed — and asserts the oracle now fails. Without that, a green
// run would only prove the command can pass, not that it measures the behavior.
//
// The vitest specs are invoked directly rather than through `pnpm` so the oracle
// is one process with one exit code, and so this script stays usable from any
// working directory.

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WEB = join(ROOT, 'apps', 'web');
const VITEST = join(WEB, 'node_modules', '.bin', 'vitest');

const SPECS = ['src/lib/server/taskStatus.test.ts', 'src/lib/server/taskService.status.test.ts'];
const RULES = join(WEB, 'src', 'lib', 'server', 'taskStatus.ts');

/** Run the two status specs. Returns the exit status and combined output. */
function runOracle() {
  const run = spawnSync(VITEST, ['run', ...SPECS], {
    cwd: WEB,
    encoding: 'utf8',
    env: { ...process.env, CI: 'true' }
  });
  return {
    ok: run.status === 0,
    output: `${run.stdout ?? ''}${run.stderr ?? ''}`,
    status: run.status
  };
}

function checkRepo() {
  const result = runOracle();
  if (!result.ok) {
    console.error('FAIL: the web status oracle did not pass');
    console.error(result.output.trim().split('\n').slice(-25).join('\n'));
    return false;
  }
  const passed = /Tests\s+\d+ passed/.test(result.output);
  if (!passed) {
    console.error('FAIL: the oracle exited zero but reported no passing tests');
    console.error(result.output.trim().split('\n').slice(-15).join('\n'));
    return false;
  }
  return true;
}

/**
 * Break the stop rule so a stopped task keeps In Progress, then confirm the
 * oracle rejects it. The original source is restored even if the run throws,
 * so a failed control cannot leave the working tree modified.
 */
function selfTest() {
  const original = readFileSync(RULES, 'utf8');
  const mutated = original.replace(
    "if (isRunning) return '21';\n\tif (isDone) return '31';\n\treturn '11';",
    "if (isRunning) return '21';\n\tif (isDone) return '31';\n\treturn '21';"
  );
  if (mutated === original) {
    console.error('FAIL: control could not mutate the rule — the source shape changed');
    return false;
  }

  let ok = false;
  try {
    writeFileSync(RULES, mutated);
    const result = runOracle();
    if (result.ok) {
      console.error('FAIL: the oracle accepted a stop that leaves In Progress');
    } else {
      ok = true;
    }
  } catch (error) {
    console.error(`FAIL: control crashed: ${error.message}`);
  } finally {
    writeFileSync(RULES, original);
  }

  // Confirm the restore, so a green control also means the tree is clean.
  if (readFileSync(RULES, 'utf8') !== original) {
    console.error('FAIL: control did not restore the source file');
    return false;
  }
  return ok;
}

const selfTestMode = process.argv.includes('--self-test');
const passed = selfTestMode ? selfTest() : checkRepo();

if (passed) {
  console.log(selfTestMode ? 'web status control passed' : 'web status verification passed');
  process.exit(0);
}
process.exit(1);
