// Verifies the TUI crate is clippy-clean under `-D warnings`, and that the same
// detector rejects a warning transcript instead of passing on anything.
//
//   node scripts/verify-clippy-clean.mjs             # G1: no warnings at all
//   node scripts/verify-clippy-clean.mjs --self-test # G2: a warning is rejected
//
// `-D warnings` makes every warning a hard error, so the crate's own exit code
// is part of the oracle rather than a count a human reads off the screen. The
// warning count is checked as well, because a transcript can carry a warning
// that `cargo` still exits zero on — a `--cap-lints` dependency, for instance.
//
// The control builds a throwaway crate that is guaranteed to warn and runs the
// same predicate over its transcript, then over the same crate with the lint
// silenced. It must reject the first and accept the second, which is what makes
// a green G1 mean the detector discriminates rather than that it cannot fail.

import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PACKAGE = 'task-timer-tui';

/**
 * The oracle, as a pure predicate over one clippy run so the control measures a
 * real transcript with the same rule rather than a lookalike.
 *
 * `-D warnings` promotes every lint to a hard error, so a lint appears as an
 * `error:` line and not a `warning:` one. Both are counted: the count is what
 * makes the check independent of `cargo`'s exit code, which a `--cap-lints`
 * dependency could otherwise satisfy while still printing a lint.
 */
function clippyIsClean({ exitCode, output }) {
  const lints = (output.match(/^(?:warning|error):/gm) ?? []).length;
  return exitCode === 0 && lints === 0;
}

function lintCount(output) {
  return (output.match(/^(?:warning|error):/gm) ?? []).length;
}

function runClippy(cwd, extraArgs = []) {
  const result = spawnSync(
    'cargo',
    ['clippy', ...extraArgs, '--', '-D', 'warnings'],
    { cwd, encoding: 'utf8', timeout: 600_000 }
  );
  return {
    exitCode: result.status,
    output: `${result.stdout ?? ''}${result.stderr ?? ''}`,
    error: result.error,
  };
}

function report(run) {
  if (run.error) return `clippy could not start: ${run.error.message}`;
  const lints = lintCount(run.output);
  if (lints > 0) {
    const first = run.output.split('\n').find((line) => /^(?:warning|error):/.test(line)) ?? '';
    return `${lints} lint line(s), first: ${first.trim()}`;
  }
  if (run.exitCode !== 0) return `clippy exited ${run.exitCode} with no lint line`;
  return 'clean';
}

function checkRepo() {
  const run = runClippy(ROOT, ['-p', PACKAGE, '--all-targets', '--all-features']);
  if (clippyIsClean(run)) return true;
  console.error(`FAIL: ${PACKAGE} is not clippy-clean — ${report(run)}`);
  return false;
}

/**
 * A crate that warns under clippy, and the same crate with the lint silenced.
 * Both transcripts are real, so the control does not depend on a hand-written
 * fixture that could drift away from clippy's actual output shape.
 *
 * The body avoids `vec!` so `clippy::useless_vec` cannot fire alongside the lint
 * under test; the control needs exactly one lint to be attributable.
 */
function controlCrate() {
  const dir = mkdtempSync(join(tmpdir(), 'clippy-control-'));
  mkdirSync(join(dir, 'src'), { recursive: true });
  writeFileSync(
    join(dir, 'Cargo.toml'),
    ['[package]', 'name = "clippy-control"', 'version = "0.0.0"', 'edition = "2021"', ''].join('\n')
  );
  // `filter_map` with an always-`Some` closure is clippy::unnecessary_filter_map
  // — the same lint this gate exists to catch in the real crate.
  const body = (allow) =>
    [
      allow ? '#![allow(clippy::unnecessary_filter_map)]' : '',
      'fn main() {',
      '    let values = [1, 2, 3];',
      '    let doubled: Vec<i32> = values.iter().filter_map(|v| Some(v * 2)).collect();',
      '    println!("{doubled:?}");',
      '}',
      '',
    ]
      .filter((line) => line !== '')
      .join('\n');
  return {
    dir,
    write: (allow) => writeFileSync(join(dir, 'src', 'main.rs'), body(allow)),
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}

function selfTest() {
  const crate = controlCrate();
  let ok = true;
  try {
    crate.write(false);
    const warned = runClippy(crate.dir);
    if (clippyIsClean(warned)) {
      console.error(
        `FAIL: the oracle accepted a crate that should warn — it cannot fail (${report(warned)})`
      );
      ok = false;
    } else if (lintCount(warned.output) === 0) {
      console.error(
        `FAIL: the control crate produced no lint line, so the rejection was inconclusive: ${report(warned)}`
      );
      ok = false;
    }

    crate.write(true);
    const silenced = runClippy(crate.dir);
    if (!clippyIsClean(silenced)) {
      console.error(
        `FAIL: the oracle rejected a clean crate — it is not measuring warnings (${report(silenced)})`
      );
      ok = false;
    }
  } catch (error) {
    console.error(`FAIL: control crashed: ${error.message}`);
    ok = false;
  } finally {
    crate.cleanup();
  }
  return ok;
}

const selfTestMode = process.argv.includes('--self-test');
const passed = selfTestMode ? selfTest() : checkRepo();

if (passed) {
  console.log(selfTestMode ? 'clippy control passed' : 'clippy clean');
  process.exit(0);
}
process.exit(1);
