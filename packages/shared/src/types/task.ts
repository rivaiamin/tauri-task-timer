export interface Task {
    id: number | string;
    label: string;
    elapsedTime: number;
    position?: number;
    isRunning?: boolean;
    startTime?: number | string | Date;
    userId?: string;
    createdAt?: string;
    updatedAt?: string;
  }
  
  // Database task (for Supabase)
  export interface DatabaseTask {
    id: number;
    user_id: string;
    label: string;
    elapsed_time: number;
    position: number;
    is_running: boolean;
    start_time: string | null;
    created_at: string;
    updated_at: string;
  }