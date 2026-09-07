import type { PageServerLoad } from './$types';
import { listTasks, getTimerMode } from '$lib/server/taskService';

function todayISO(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

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
