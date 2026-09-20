export interface Task {
    id: number | string;
    label: string;
    code?: string | null;
    description?: string | null;
    link?: string | null;
    status?: string;
    notes?: string | null;
    tags?: string[] | null;
    /** The viewed day's row id, when the task came from a day-scoped query. */
    dayId?: number | null;
    /** The viewed day (YYYY-MM-DD); null on an archive query. */
    workDate?: string | null;
    elapsedTime: number;
    totalTime?: number;
    position?: number;
    isRunning?: boolean;
    done?: boolean;
    isCompleted?: boolean;
    isCancelled?: boolean;
    isDeleted?: boolean;
    isArchived?: boolean;
    isPinned?: boolean;
    isImportant?: boolean;
    startTime?: number | string | Date;
    endTime?: number | string | Date | null;
    userId?: string;
    createdAt?: string;
    updatedAt?: string;
    /** Archive only: every day the task was worked. */
    days?: TaskDay[];
  }

  /** One day of work on a task. */
  export interface TaskDay {
    id: number;
    workDate: string;
    status: string;
    elapsedTime: number;
    totalTime: number;
    position: number;
    isRunning: boolean;
    done: boolean;
    isCompleted: boolean;
    isCancelled: boolean;
    isDeleted: boolean;
    isArchived: boolean;
    isPinned: boolean;
    isImportant: boolean;
    startTime: number | string | Date | null;
    endTime: number | string | Date | null;
  }

  // Task identity row (database shape). One per (user_id, label).
  export interface DatabaseTask {
    id: number;
    user_id: string;
    label: string;
    code?: string | null;
    description?: string | null;
    link?: string | null;
    notes?: string | null;
    tags?: string[] | null;
    created_at: string;
    updated_at: string;
  }

  // One day's row for a task (database shape).
  export interface DatabaseTaskDay {
    id: number;
    task_id: number;
    work_date: string;
    status: string;
    elapsed_time: number;
    total_time: number;
    position: number;
    is_running: boolean;
    done: boolean;
    is_completed: boolean;
    is_cancelled: boolean;
    is_deleted: boolean;
    is_archived: boolean;
    is_pinned: boolean;
    is_important: boolean;
    start_time: string | null;
    end_time: string | null;
    created_at: string;
    updated_at: string;
  }