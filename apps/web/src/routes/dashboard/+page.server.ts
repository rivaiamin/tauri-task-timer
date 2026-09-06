import type { PageServerLoad } from './$types';
import { listTasks, getTimerMode } from '$lib/server/taskService';

export const load: PageServerLoad = async ({ locals }) => {
  const user = locals.user!; // guaranteed by +layout.server.ts
  const now = new Date();
  const workDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  const { tasks, totalElapsedSeconds } = listTasks(user.id, workDate);
  return {
    tasks,
    totalElapsedSeconds,
    timerMode: getTimerMode(user.id),
    userEmail: user.email
  };
};
