// Pure timer math shared by the web UI and the server API so their behavior
// can never drift. No I/O — takes primitives, returns numbers.

/**
 * Current elapsed seconds for a task, counting live time if it is running.
 * @param storedElapsed accumulated seconds saved on the task
 * @param isRunning whether the timer is currently running
 * @param startTimeMs epoch ms the current run started (or null/undefined)
 * @param now epoch ms "now" (injectable for testing)
 */
export function currentElapsedSeconds(
  storedElapsed: number,
  isRunning: boolean,
  startTimeMs: number | null | undefined,
  now: number = Date.now()
): number {
  if (!isRunning || !startTimeMs) return storedElapsed;
  return storedElapsed + Math.max(0, Math.floor((now - startTimeMs) / 1000));
}

/** Story points: 1 hour of tracked time = 1 point. */
export function secondsToStoryPoints(seconds: number): number {
  return seconds / 3600;
}
