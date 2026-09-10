import type { PageServerLoad } from './$types';
import { listTasks } from '$lib/server/taskService';

export const load: PageServerLoad = async ({ locals, url }) => {
  const user = locals.user!;
  const q = url.searchParams.get('q') || undefined;
  const tag = url.searchParams.get('tag') || undefined;
  const status = url.searchParams.get('status') || undefined;
  const { tasks, totalElapsedSeconds } = listTasks(user.id, { archived: true, q, tag, status });
  return { tasks, totalElapsedSeconds };
};
