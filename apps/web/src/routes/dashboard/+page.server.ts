import type { PageServerLoad } from './$types';
import { listTasks, getTimerMode } from '$lib/server/taskService';
import { todayISO } from '$lib/dates';

export const load: PageServerLoad = async ({ locals, url }) => {
  const user = locals.user!; // guaranteed by +layout.server.ts
  const param = url.searchParams.get('date');
  const workDate = param && /^\d{4}-\d{2}-\d{2}$/.test(param) ? param : todayISO();
  const { tasks, totalElapsedSeconds } = listTasks(user.id, workDate);
  return {
    tasks,
    totalElapsedSeconds,
    timerMode: getTimerMode(user.id),
    userEmail: user.email,
    workDate
  };
};
