export interface Task {
    id: number | string;
    label: string;
    code?: string | null;
    description?: string | null;
    link?: string | null;
    status?: string;
    notes?: string | null;
    tags?: string[] | null;
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
  }
  
  // Database task (for Supabase)
  export interface DatabaseTask {
    id: number;
    user_id: string;
    label: string;
    work_date: string;
    code?: string | null;
    description?: string | null;
    link?: string | null;
    status: string;
    notes?: string | null;
    tags?: string[] | null;
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