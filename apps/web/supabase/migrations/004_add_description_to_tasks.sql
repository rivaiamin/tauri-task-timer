-- Add description column to tasks table
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS description TEXT;

