import type { PageServerLoad } from './$types';
import { listTasks, getTimerMode } from '$lib/server/taskService';

export const load: PageServerLoad = async ({ locals }) => {
  const user = locals.user!; // guaranteed by +layout.server.ts
  const { tasks, totalElapsedSeconds } = listTasks(user.id);
  return {
    tasks,
    totalElapsedSeconds,
    timerMode: getTimerMode(user.id),
    userEmail: user.email
  };
};
