# Task Timer - Migration to Full-Stack Architecture

## Overview

This document outlines the migration plan to transform the Task Timer from a desktop-only application to a full-stack web application with real-time collaboration, cloud sync, and webhook integration capabilities.

## Project Structure: Monorepo Approach

This migration uses a **monorepo structure** to keep both the desktop (Tauri) and web (SvelteKit) applications in the same repository, allowing code sharing and easier maintenance.

### Monorepo Benefits
- **Shared Code**: Common utilities, types, and constants
- **Single Source of Truth**: Data models defined once
- **Easier Maintenance**: Update once, use in both apps
- **Future-Proof**: Easy to add mobile or other platforms

### Directory Structure

```
task-timer/
├── apps/
│   ├── desktop/              # Existing Tauri app
│   │   ├── src/              # Vanilla JS frontend
│   │   ├── src-tauri/        # Tauri backend (Rust)
│   │   ├── package.json
│   │   └── build.js
│   │
│   └── web/                  # New SvelteKit web app
│       ├── src/
│       │   ├── routes/
│       │   ├── lib/
│       │   └── app.html
│       ├── supabase/
│       │   └── migrations/
│       ├── package.json
│       └── svelte.config.js
│
├── packages/
│   └── shared/               # Shared code between desktop & web
│       ├── src/
│       │   ├── index.ts
│       │   ├── types/
│       │   │   └── task.ts
│       │   └── utils/
│       │       ├── formatTime.ts
│       │       └── escapeHTML.ts
│       └── package.json
│
├── package.json              # Root workspace config
├── pnpm-workspace.yaml       # Workspace configuration
└── README.md
```

## Current Architecture

- **Frontend**: Vanilla JavaScript with Tailwind CSS
- **Backend**: Tauri (Rust) with SQLite for desktop
- **Storage**: SQLite (desktop) / localStorage (web fallback)
- **Deployment**: Desktop app only

## Target Architecture

- **Frontend**: SvelteKit (full-stack framework)
- **Backend**: Supabase (PostgreSQL + Real-time + Auth)
- **Storage**: Cloud database with local caching
- **Deployment**: Vercel (web) + Tauri (desktop, synced)

## New Features Requirements

1. **View Mode**: Shareable URLs for real-time time tracking viewing
2. **Personalized Data**: Cloud sync across devices with user accounts
3. **Webhook Integration**: Secure server-side webhook delivery (Google Chat, Slack, etc.)

---

## Phase 1: Monorepo Setup & Infrastructure

### 1.1 Restructure Existing Project

```bash
# In your current project root
cd /home/amin/projects/js/task-timer

# Create new directory structure
mkdir -p apps/desktop apps/web packages/shared

# Move existing files to desktop app
mv src apps/desktop/
mv src-tauri apps/desktop/
mv build.js apps/desktop/
# Keep dist/ at root or move to apps/desktop/dist/
```

### 1.2 Create Root Workspace Configuration

Create `pnpm-workspace.yaml` in project root:

```yaml
packages:
  - 'apps/*'
  - 'packages/*'
```

Update root `package.json`:

```json
{
  "name": "task-timer-monorepo",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev:desktop": "pnpm --filter desktop dev",
    "dev:web": "pnpm --filter web dev",
    "build:desktop": "pnpm --filter desktop build",
    "build:web": "pnpm --filter web build",
    "build:all": "pnpm build:desktop && pnpm build:web"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.9.4"
  }
}
```

### 1.3 Create Desktop App Package

Create `apps/desktop/package.json`:

```json
{
  "name": "desktop",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "tauri": "tauri",
    "dev": "tauri dev",
    "build": "node build.js",
    "preview": "npm run build && npx http-server dist"
  },
  "dependencies": {
    "shared": "workspace:*"
  }
}
```

### 1.4 Create Shared Package

Create `packages/shared/package.json`:

```json
{
  "name": "shared",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "exports": {
    ".": "./src/index.ts",
    "./types": "./src/types/index.ts",
    "./utils": "./src/utils/index.ts"
  }
}
```

Create `packages/shared/src/utils/formatTime.ts`:

```typescript
export function formatTime(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  return [hours, minutes, seconds]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
}
```

Create `packages/shared/src/utils/escapeHTML.ts`:

```typescript
export function escapeHTML(str: string): string {
  const div = document.createElement("div");
  div.appendChild(document.createTextNode(str));
  return div.innerHTML;
}
```

Create `packages/shared/src/types/task.ts`:

```typescript
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
```

Create `packages/shared/src/index.ts`:

```typescript
export * from './types/task';
export * from './utils/formatTime';
export * from './utils/escapeHTML';
```

### 1.5 Create SvelteKit Web App

```bash
# Create new SvelteKit project in apps/web
cd apps
pnpm create svelte@latest web
cd web
```

**Important**: After creating the SvelteKit project, you need to:

1. First, update `apps/web/package.json` to include the shared package and other dependencies (see below)
2. Then run `pnpm install` from the **project root** (not from `apps/web/`) to link workspace packages

Create `apps/web/package.json` (update the generated one to include shared and other dependencies):

```json
{
  "name": "web",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "@supabase/supabase-js": "^2.39.0",
    "@supabase/auth-helpers-sveltekit": "^0.8.0",
    "shared": "workspace:*",
    "zod": "^3.22.0"
  },
  "devDependencies": {
    "@sveltejs/adapter-vercel": "^3.0.0",
    "@sveltejs/kit": "^1.27.0",
    "svelte": "^4.2.0",
    "vite": "^5.0.0"
  }
}
```

**Important**: After updating `package.json` with the dependencies above, run `pnpm install` from the **project root** (not from `apps/web/`) to link the workspace packages:

```bash
# From project root (where pnpm-workspace.yaml is located)
cd ../..  # if you're in apps/web, go back to root
pnpm install
```

This will:
- Install all dependencies for all packages
- Automatically link the `shared` workspace package to `apps/web`
- Set up the workspace correctly

### 1.6 Set Up Supabase Project

1. Go to [database.new](https://database.new) and create a new Supabase project
   - Alternatively, create a project via the [Supabase Dashboard](https://supabase.com/dashboard)
2. Once your project is created, go to the project's **Settings** → **API** to get your keys
3. Note down:
   - **Project URL** (found in Project Settings → API)
   - **Publishable key** (new format: `sb_publishable_xxx`) - for client-side operations
   - **Service role key** (legacy format, or new format) - for server-side operations (keep secret!)

**Note**: Supabase is transitioning to new API keys. The new **publishable key** (format: `sb_publishable_xxx`) will replace the older `anon` key. During the transition period, both work, but the new publishable key is recommended. You can find all keys in the **API Keys** section of your project's Settings page.

**Reference**: For the latest Supabase setup instructions, see the [official SvelteKit quickstart guide](https://supabase.com/docs/guides/getting-started/quickstarts/sveltekit).

### 1.7 Environment Variables

Create `apps/web/.env`:

```env
PUBLIC_SUPABASE_URL=your-project-url
PUBLIC_SUPABASE_PUBLISHABLE_KEY=your-publishable-key
SUPABASE_SERVICE_ROLE_KEY=your-service-role-key
ENCRYPTION_KEY=your-32-byte-encryption-key  # For webhook URLs
```

**Note**: If you're using legacy keys during the transition, you can use `PUBLIC_SUPABASE_ANON_KEY` instead of `PUBLIC_SUPABASE_PUBLISHABLE_KEY`, but the new publishable key is recommended.

Create `apps/web/.env.example`:

```env
PUBLIC_SUPABASE_URL=
PUBLIC_SUPABASE_PUBLISHABLE_KEY=
SUPABASE_SERVICE_ROLE_KEY=
ENCRYPTION_KEY=
```

**Where to find your keys:**
- **Project URL** and **Publishable key**: Found in the project's **Connect** dialog or Settings → API → API Keys
- **Service role key**: Found in Settings → API → API Keys (Legacy API Keys tab or new API Keys tab)
- All keys are also available in Settings → API → API Keys section

**Generating ENCRYPTION_KEY**: This key is used to encrypt webhook URLs before storing them in the database. Generate a secure 32-byte key:

```bash
# Option 1: Using Node.js (recommended)
node -e "console.log(require('crypto').randomBytes(32).toString('base64'))"

# Option 2: Using OpenSSL
openssl rand -base64 32
```

Copy the output and paste it as the value for `ENCRYPTION_KEY` in your `.env` file. **Important**: Keep this key secret and never commit it to git. If you change this key, existing encrypted webhook URLs cannot be decrypted.

### 1.8 Install All Dependencies

```bash
# From project root (where pnpm-workspace.yaml is located)
# This will install dependencies for all packages and link workspace packages
pnpm install
```

**Important**: Make sure you've added `"shared": "workspace:*"` to `apps/web/package.json` dependencies before running this command. The workspace packages are automatically linked when you run `pnpm install` from the root - you don't need to explicitly install them.

### 1.9 Updated Project Structure

```
task-timer/
├── apps/
│   ├── desktop/
│   │   ├── src/
│   │   │   ├── main.js          # Uses shared utilities
│   │   │   ├── index.html
│   │   │   └── styles.css
│   │   ├── src-tauri/
│   │   │   └── src/
│   │   │       └── lib.rs        # Can sync with Supabase
│   │   ├── package.json
│   │   └── build.js
│   │
│   └── web/
│       ├── src/
│       │   ├── lib/
│       │   │   ├── supabaseClient.ts    # Main Supabase client (client-side)
│       │   │   ├── supabaseServer.ts    # Server-side Supabase client
│       │   │   ├── auth/
│       │   │   │   └── helpers.ts
│       │   │   ├── encryption/
│       │   │   │   └── webhook.ts
│       │   │   └── types/
│       │   │       └── database.ts
│       │   ├── routes/
│       │   │   ├── (auth)/
│       │   │   │   ├── login/
│       │   │   │   │   └── +page.svelte
│       │   │   │   └── register/
│       │   │   │       └── +page.svelte
│       │   │   ├── dashboard/
│       │   │   │   └── +page.svelte
│       │   │   ├── view/
│       │   │   │   └── [token]/
│       │   │   │       └── +page.svelte
│       │   │   └── api/
│       │   │       ├── tasks/
│       │   │       ├── webhooks/
│       │   │       └── share/
│       │   └── app.html
│       ├── supabase/
│       │   └── migrations/
│       │       ├── 001_initial_schema.sql
│       │       ├── 002_rls_policies.sql
│       │       └── 003_functions.sql
│       ├── package.json
│       └── svelte.config.js
│
├── packages/
│   └── shared/
│       ├── src/
│       │   ├── index.ts
│       │   ├── types/
│       │   │   └── task.ts
│       │   └── utils/
│       │       ├── formatTime.ts
│       │       └── escapeHTML.ts
│       └── package.json
│
├── package.json
├── pnpm-workspace.yaml
└── README.md
```

---

## Phase 2: Database Schema & Migrations

### 2.1 Initial Schema Migration

Create `apps/web/supabase/migrations/001_initial_schema.sql`:

```sql
-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- User profiles (extends Supabase auth.users)
CREATE TABLE IF NOT EXISTS user_profiles (
  id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
  username TEXT UNIQUE,
  display_name TEXT,
  avatar_url TEXT,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Tasks table
CREATE TABLE IF NOT EXISTS tasks (
  id BIGSERIAL PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  label TEXT NOT NULL,
  elapsed_time INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0,
  is_running BOOLEAN DEFAULT FALSE,
  start_time TIMESTAMP WITH TIME ZONE,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Shareable view links
CREATE TABLE IF NOT EXISTS view_links (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  share_token TEXT UNIQUE NOT NULL,
  is_active BOOLEAN DEFAULT TRUE,
  expires_at TIMESTAMP WITH TIME ZONE,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Webhooks (secured, server-side only)
CREATE TABLE IF NOT EXISTS webhooks (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  url_encrypted TEXT NOT NULL, -- Encrypted webhook URL
  name TEXT,
  description TEXT,
  is_active BOOLEAN DEFAULT TRUE,
  last_triggered_at TIMESTAMP WITH TIME ZONE,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_tasks_user_id ON tasks(user_id);
CREATE INDEX IF NOT EXISTS idx_tasks_position ON tasks(user_id, position);
CREATE INDEX IF NOT EXISTS idx_view_links_token ON view_links(share_token);
CREATE INDEX IF NOT EXISTS idx_view_links_user_id ON view_links(user_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_user_id ON webhooks(user_id);

-- Updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply updated_at triggers
CREATE TRIGGER update_user_profiles_updated_at BEFORE UPDATE ON user_profiles
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tasks_updated_at BEFORE UPDATE ON tasks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_webhooks_updated_at BEFORE UPDATE ON webhooks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

### 2.2 Row Level Security (RLS) Policies

Create `apps/web/supabase/migrations/002_rls_policies.sql`:

```sql
-- Enable Row Level Security
ALTER TABLE user_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE view_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhooks ENABLE ROW LEVEL SECURITY;

-- User Profiles Policies
CREATE POLICY "Users can view own profile" ON user_profiles
  FOR SELECT USING (auth.uid() = id);

CREATE POLICY "Users can update own profile" ON user_profiles
  FOR UPDATE USING (auth.uid() = id);

CREATE POLICY "Users can insert own profile" ON user_profiles
  FOR INSERT WITH CHECK (auth.uid() = id);

-- Tasks Policies
CREATE POLICY "Users can view own tasks" ON tasks
  FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own tasks" ON tasks
  FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own tasks" ON tasks
  FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own tasks" ON tasks
  FOR DELETE USING (auth.uid() = user_id);

-- View Links: Users can view their own links
CREATE POLICY "Users can view own view links" ON view_links
  FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can create own view links" ON view_links
  FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own view links" ON view_links
  FOR UPDATE USING (auth.uid() = user_id);

-- View Links: Public read access with valid token (for shareable views)
-- This requires a custom function to check token validity
CREATE OR REPLACE FUNCTION check_view_token(token TEXT)
RETURNS UUID AS $$
DECLARE
  user_uuid UUID;
BEGIN
  SELECT user_id INTO user_uuid
  FROM view_links
  WHERE share_token = token
    AND is_active = TRUE
    AND (expires_at IS NULL OR expires_at > NOW());
  
  RETURN user_uuid;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Webhooks Policies (strict - only own webhooks)
CREATE POLICY "Users can view own webhooks" ON webhooks
  FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own webhooks" ON webhooks
  FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own webhooks" ON webhooks
  FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own webhooks" ON webhooks
  FOR DELETE USING (auth.uid() = user_id);
```

### 2.3 Database Functions

Create `apps/web/supabase/migrations/003_functions.sql`:

```sql
-- Function to generate share token
CREATE OR REPLACE FUNCTION generate_share_token()
RETURNS TEXT AS $$
BEGIN
  RETURN encode(gen_random_bytes(32), 'base64');
END;
$$ LANGUAGE plpgsql;

-- Function to get tasks for view token (for public viewing)
CREATE OR REPLACE FUNCTION get_tasks_for_view(token TEXT)
RETURNS TABLE (
  id BIGINT,
  label TEXT,
  elapsed_time INTEGER,
  position INTEGER,
  is_running BOOLEAN,
  start_time TIMESTAMP WITH TIME ZONE
) AS $$
DECLARE
  target_user_id UUID;
BEGIN
  -- Verify token and get user_id
  SELECT user_id INTO target_user_id
  FROM view_links
  WHERE share_token = token
    AND is_active = TRUE
    AND (expires_at IS NULL OR expires_at > NOW());
  
  IF target_user_id IS NULL THEN
    RAISE EXCEPTION 'Invalid or expired token';
  END IF;
  
  -- Return tasks for that user
  RETURN QUERY
  SELECT t.id, t.label, t.elapsed_time, t.position, t.is_running, t.start_time
  FROM tasks t
  WHERE t.user_id = target_user_id
  ORDER BY t.position, t.id;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 2.4 Apply Migrations

```bash
# Using Supabase CLI
supabase db push

# Or manually via Supabase dashboard SQL editor
```

---

## Phase 3: Frontend Migration

### 3.1 Supabase Client Setup

Create `apps/web/src/lib/supabaseClient.ts` (or `supabaseClient.js`):

```typescript
import { createClient } from '@supabase/supabase-js';
import { PUBLIC_SUPABASE_URL, PUBLIC_SUPABASE_PUBLISHABLE_KEY } from '$env/static/public';

export const supabase = createClient(
  PUBLIC_SUPABASE_URL,
  PUBLIC_SUPABASE_PUBLISHABLE_KEY
);
```

**Note**: If you're using legacy keys during the transition period, you can use `PUBLIC_SUPABASE_ANON_KEY` instead of `PUBLIC_SUPABASE_PUBLISHABLE_KEY`, but the new publishable key is recommended. The new publishable key format is `sb_publishable_xxx` and provides better security.

Create `apps/web/src/lib/supabaseServer.ts` (or `supabaseServer.js`):

```typescript
import { createClient } from '@supabase/supabase-js';
import { PUBLIC_SUPABASE_URL } from '$env/static/public';
import { SUPABASE_SERVICE_ROLE_KEY } from '$env/static/private';

// Server-side client with service role (for admin operations)
// This bypasses Row Level Security (RLS) policies
export const supabaseAdmin = createClient(
  PUBLIC_SUPABASE_URL,
  SUPABASE_SERVICE_ROLE_KEY,
  {
    auth: {
      autoRefreshToken: false,
      persistSession: false
    }
  }
);
```

**Important**: The service role key should **never** be exposed to the client. It bypasses all RLS policies and should only be used in server-side code (API routes, server load functions, etc.).

### 3.2 Auth Helpers

Create `apps/web/src/lib/auth/helpers.ts`:

```typescript
import { supabase } from '$lib/supabaseClient';
import type { Session } from '@supabase/supabase-js';

export async function getSession(): Promise<Session | null> {
  const { data: { session } } = await supabase.auth.getSession();
  return session;
}

export async function signIn(email: string, password: string) {
  const { data, error } = await supabase.auth.signInWithPassword({
    email,
    password
  });
  return { data, error };
}

export async function signUp(email: string, password: string) {
  const { data, error } = await supabase.auth.signUp({
    email,
    password
  });
  return { data, error };
}

export async function signOut() {
  const { error } = await supabase.auth.signOut();
  return { error };
}
```

### 3.3 Webhook Encryption

Create `apps/web/src/lib/encryption/webhook.ts`:

```typescript
import { ENCRYPTION_KEY } from '$env/static/private';
import crypto from 'crypto';

const ALGORITHM = 'aes-256-gcm';
const IV_LENGTH = 16;
const SALT_LENGTH = 64;
const TAG_LENGTH = 16;
const TAG_POSITION = SALT_LENGTH + IV_LENGTH;
const ENCRYPTED_POSITION = TAG_POSITION + TAG_LENGTH;

function getKey(): Buffer {
  return crypto.scryptSync(ENCRYPTION_KEY, 'salt', 32);
}

export function encryptWebhookUrl(text: string): string {
  const iv = crypto.randomBytes(IV_LENGTH);
  const key = getKey();
  
  const cipher = crypto.createCipheriv(ALGORITHM, key, iv);
  
  let encrypted = cipher.update(text, 'utf8');
  encrypted = Buffer.concat([encrypted, cipher.final()]);
  
  const tag = cipher.getAuthTag();
  
  return Buffer.concat([
    Buffer.from('salt'), // Salt placeholder
    iv,
    tag,
    encrypted
  ]).toString('base64');
}

export function decryptWebhookUrl(encryptedData: string): string {
  const data = Buffer.from(encryptedData, 'base64');
  
  const iv = data.slice(SALT_LENGTH, TAG_POSITION);
  const tag = data.slice(TAG_POSITION, ENCRYPTED_POSITION);
  const encrypted = data.slice(ENCRYPTED_POSITION);
  
  const key = getKey();
  const decipher = crypto.createDecipheriv(ALGORITHM, key, iv);
  
  decipher.setAuthTag(tag);
  
  let decrypted = decipher.update(encrypted);
  decrypted = Buffer.concat([decrypted, decipher.final()]);
  
  return decrypted.toString('utf8');
}
```

### 3.4 Convert Main Timer Component

This section converts the vanilla JavaScript timer app (`apps/desktop/src/main.js`) into a Svelte component. The original app has the following features that need to be preserved:

- Task management (add, delete, edit, reorder)
- Timer functionality (start/stop, reset, edit time)
- Real-time sync via Supabase
- CSV export
- 8-hour sound notification
- Total time calculation
- Responsive UI with Tailwind CSS

#### 3.4.1 Create Dashboard Layout

First, create the layout file `apps/web/src/routes/dashboard/+layout.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { getSession } from '$lib/auth/helpers';
  import { goto } from '$app/navigation';

  let session = await getSession();

  onMount(() => {
    if (!session) {
      goto('/login');
    }
  });
</script>

{#if session}
  <slot />
{:else}
  <div class="flex items-center justify-center min-h-screen">
    <p>Redirecting to login...</p>
  </div>
{/if}
```

#### 3.4.2 Create Main Dashboard Page

Create `apps/web/src/routes/dashboard/+page.svelte` with the complete timer functionality:

```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { supabase } from '$lib/supabaseClient';
  import { formatTime, escapeHTML } from 'shared';
  import type { DatabaseTask } from 'shared';
  import { getSession } from '$lib/auth/helpers';

  let session = await getSession();
  let tasks: DatabaseTask[] = [];
  let totalTime = 0;
  let taskInput = '';
  let totalTimerInterval: ReturnType<typeof setInterval> | null = null;
  let hasPlayed8HourSound = false;
  let channel: any = null;

  // Reactive: Update total time whenever tasks change
  $: {
    totalTime = getTotalTime();
    check8HourNotification();
  }

  onMount(async () => {
    if (!session) return;

    await loadTasks();

    // Subscribe to real-time updates
    channel = supabase
      .channel('tasks-changes')
      .on(
        'postgres_changes',
        {
          event: '*',
          schema: 'public',
          table: 'tasks',
          filter: `user_id=eq.${session.user.id}`
        },
        (payload: any) => {
          handleRealtimeUpdate(payload);
        }
      )
      .subscribe();

    // Start total timer interval
    totalTimerInterval = setInterval(() => {
      totalTime = getTotalTime();
    }, 1000);

    // Handle visibility change (for background tab issue)
    document.addEventListener('visibilitychange', handleVisibilityChange);
  });

  onDestroy(() => {
    if (channel) {
      supabase.removeChannel(channel);
    }
    if (totalTimerInterval) {
      clearInterval(totalTimerInterval);
    }
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  });

  async function loadTasks() {
    if (!session) return;

    const { data, error } = await supabase
      .from('tasks')
      .select('*')
      .eq('user_id', session.user.id)
      .order('position', { ascending: true });

    if (error) {
      console.error('Error loading tasks:', error);
      return;
    }

    tasks = (data || []).map(task => ({
      ...task,
      // Add client-side only properties
      intervalId: null as any,
      startTime: task.start_time ? new Date(task.start_time).getTime() : null
    }));
  }

  function handleRealtimeUpdate(payload: any) {
    if (payload.eventType === 'INSERT') {
      tasks = [...tasks, {
        ...payload.new,
        intervalId: null,
        startTime: payload.new.start_time ? new Date(payload.new.start_time).getTime() : null
      }];
    } else if (payload.eventType === 'UPDATE') {
      tasks = tasks.map(task => 
        task.id === payload.new.id 
          ? {
              ...payload.new,
              intervalId: task.intervalId,
              startTime: payload.new.start_time ? new Date(payload.new.start_time).getTime() : task.startTime
            }
          : task
      );
    } else if (payload.eventType === 'DELETE') {
      tasks = tasks.filter(t => t.id !== payload.old.id);
    }
  }

  function getCurrentElapsedTime(task: DatabaseTask & { startTime: number | null }): number {
    if (!task.is_running || !task.startTime) {
      return task.elapsed_time;
    }
    const elapsedSinceStart = Math.floor((Date.now() - task.startTime) / 1000);
    return task.elapsed_time + elapsedSinceStart;
  }

  function getTotalTime(): number {
    return tasks.reduce((sum, task) => {
      return sum + getCurrentElapsedTime(task as any);
    }, 0);
  }

  function check8HourNotification() {
    if (totalTime >= 28800 && !hasPlayed8HourSound) {
      play8HourSound();
      hasPlayed8HourSound = true;
    } else if (totalTime < 28800) {
      hasPlayed8HourSound = false;
    }
  }

  function play8HourSound() {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    oscillator.frequency.value = 800;
    oscillator.type = 'sine';
    
    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.5);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.5);
  }

  async function addTask(event: Event) {
    event.preventDefault();
    const label = taskInput.trim();
    if (!label || !session) return;

    const { data, error } = await supabase
      .from('tasks')
      .insert({
        user_id: session.user.id,
        label,
        elapsed_time: 0,
        position: tasks.length,
        is_running: false
      })
      .select()
      .single();

    if (error) {
      console.error('Error adding task:', error);
      alert('Failed to add task');
      return;
    }

    tasks = [...tasks, {
      ...data,
      intervalId: null,
      startTime: null
    }];
    taskInput = '';
  }

  async function toggleTimer(id: number) {
    if (!session) return;

    const taskToToggle = tasks.find(t => t.id === id);
    if (!taskToToggle) return;

    const isStarting = !taskToToggle.is_running;

    // Stop all other running timers
    if (isStarting) {
      for (const task of tasks) {
        if (task.is_running && task.id !== id) {
          await stopTimer(task.id);
        }
      }
    }

    if (isStarting) {
      await startTimer(id);
    } else {
      await stopTimer(id);
    }
  }

  async function startTimer(id: number) {
    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        is_running: true,
        start_time: new Date().toISOString()
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error starting timer:', error);
      return;
    }

    // Update local state
    tasks = tasks.map(task => 
      task.id === id 
        ? { ...task, is_running: true, startTime: Date.now() }
        : task
    );
  }

  async function stopTimer(id: number) {
    if (!session) return;

    const task = tasks.find(t => t.id === id);
    if (!task) return;

    const currentElapsed = getCurrentElapsedTime(task as any);

    const { error } = await supabase
      .from('tasks')
      .update({
        is_running: false,
        elapsed_time: currentElapsed,
        start_time: null
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error stopping timer:', error);
      return;
    }

    // Update local state
    tasks = tasks.map(t => 
      t.id === id 
        ? { ...t, is_running: false, elapsed_time: currentElapsed, startTime: null }
        : t
    );
  }

  async function resetTimer(id: number) {
    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        elapsed_time: 0,
        is_running: false,
        start_time: null
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error resetting timer:', error);
      return;
    }

    tasks = tasks.map(t => 
      t.id === id 
        ? { ...t, elapsed_time: 0, is_running: false, startTime: null }
        : t
    );
  }

  async function deleteTask(id: number) {
    if (!session) return;
    if (!confirm('Are you sure you want to delete this task?')) return;

    const { error } = await supabase
      .from('tasks')
      .delete()
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error deleting task:', error);
      return;
    }

    tasks = tasks.filter(t => t.id !== id);
  }

  async function moveTaskUp(id: number) {
    const index = tasks.findIndex(t => t.id === id);
    if (index <= 0 || !session) return;

    const newPosition = index - 1;
    await reorderTasks(id, newPosition);
  }

  async function moveTaskDown(id: number) {
    const index = tasks.findIndex(t => t.id === id);
    if (index < 0 || index >= tasks.length - 1 || !session) return;

    const newPosition = index + 1;
    await reorderTasks(id, newPosition);
  }

  async function reorderTasks(taskId: number, newPosition: number) {
    if (!session) return;

    // Swap positions
    const task1 = tasks[newPosition];
    const task2 = tasks.find(t => t.id === taskId);
    if (!task2) return;

    // Update both tasks' positions
    const updates = [
      supabase.from('tasks').update({ position: newPosition }).eq('id', taskId).eq('user_id', session.user.id),
      supabase.from('tasks').update({ position: task1.position }).eq('id', task1.id).eq('user_id', session.user.id)
    ];

    await Promise.all(updates);

    // Update local state
    const newTasks = [...tasks];
    [newTasks[newPosition], newTasks[tasks.findIndex(t => t.id === taskId)]] = 
      [newTasks[tasks.findIndex(t => t.id === taskId)], newTasks[newPosition]];
    tasks = newTasks;
  }

  function parseTimeInput(value: string): number | null {
    const input = value.trim();
    if (!input) return null;

    // Support HH:MM:SS, MM:SS, or SS
    if (/^\d+(:\d+){0,2}$/.test(input)) {
      const parts = input.split(':').map(Number);
      if (parts.some(n => isNaN(n) || n < 0)) return null;

      let hours = 0, minutes = 0, seconds = 0;
      if (parts.length === 3) {
        [hours, minutes, seconds] = parts;
      } else if (parts.length === 2) {
        [minutes, seconds] = parts;
      } else {
        [seconds] = parts;
      }

      return hours * 3600 + minutes * 60 + seconds;
    }

    // Fallback: treat as minutes (can be decimal)
    const asNumber = Number(input.replace(',', '.'));
    if (!isFinite(asNumber) || asNumber < 0) return null;
    return Math.round(asNumber * 60);
  }

  async function editTaskTime(id: number) {
    if (!session) return;

    const task = tasks.find(t => t.id === id);
    if (!task) return;

    const currentSeconds = getCurrentElapsedTime(task as any);
    const currentFormatted = formatTime(currentSeconds);

    const input = prompt(
      `Edit time for "${task.label}".\n\nUse HH:MM:SS (e.g. 01:30:00) or minutes (e.g. 90 for 1.5 hours).`,
      currentFormatted
    );

    if (input === null) return;

    const newSeconds = parseTimeInput(input);
    if (newSeconds === null) {
      alert("Couldn't understand that time. Please use HH:MM:SS or minutes.");
      return;
    }

    const { error } = await supabase
      .from('tasks')
      .update({
        elapsed_time: newSeconds,
        start_time: task.is_running ? new Date().toISOString() : null
      })
      .eq('id', id)
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error updating task time:', error);
      return;
    }

    tasks = tasks.map(t => 
      t.id === id 
        ? { ...t, elapsed_time: newSeconds, startTime: t.is_running ? Date.now() : null }
        : t
    );
  }

  async function resetAllTimers() {
    if (tasks.length === 0) {
      alert('No tasks to reset.');
      return;
    }

    if (!confirm('Are you sure you want to reset all timers to 00:00:00?')) {
      return;
    }

    if (!session) return;

    const { error } = await supabase
      .from('tasks')
      .update({
        elapsed_time: 0,
        is_running: false,
        start_time: null
      })
      .eq('user_id', session.user.id);

    if (error) {
      console.error('Error resetting all timers:', error);
      return;
    }

    tasks = tasks.map(t => ({
      ...t,
      elapsed_time: 0,
      is_running: false,
      startTime: null
    }));
  }

  function tasksToCsv(): string {
    const header = ['Task', 'Story Points'];
    const rows = [header];

    tasks.forEach(task => {
      const storyPoints = getCurrentElapsedTime(task as any) / 3600;
      rows.push([task.label, storyPoints.toFixed(2)]);
    });

    return rows
      .map(row =>
        row
          .map(field => {
            const value = String(field ?? '');
            const escaped = value.replace(/"/g, '""');
            return `"${escaped}"`;
          })
          .join(',')
      )
      .join('\r\n');
  }

  function exportTasksAsCsv() {
    if (!tasks.length) {
      alert('No tasks to export yet.');
      return;
    }

    const csvContent = tasksToCsv();
    const datePart = new Date().toISOString().slice(0, 10);
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);

    const link = document.createElement('a');
    link.href = url;
    link.download = `tasks-${datePart}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }

  function handleVisibilityChange() {
    if (!document.hidden) {
      // Page became visible - update all running timers
      tasks = tasks.map(task => task); // Trigger reactivity
    }
  }
</script>

<div class="min-h-screen p-4 sm:p-8">
  <div class="max-w-2xl mx-auto bg-white rounded-xl shadow-lg p-6 sm:p-8">
    <header class="mb-6">
      <div class="flex items-center justify-between">
        <div class="flex-1">
          <h1 class="text-3xl font-bold text-gray-900 text-center sm:text-left">
            Task Timer
          </h1>
          <p class="text-center sm:text-left text-gray-500 mt-1">
            Add tasks and track your time. Only one timer can run at a time.
          </p>
        </div>
        <div class="hidden sm:flex items-center gap-2 ml-4">
          <button
            on:click={resetAllTimers}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-orange-600 rounded-lg shadow-sm hover:bg-orange-700 transition-colors"
            type="button"
          >
            Reset All
          </button>
          <button
            on:click={exportTasksAsCsv}
            class="inline-flex items-center px-3 py-2 text-sm font-semibold text-white bg-emerald-600 rounded-lg shadow-sm hover:bg-emerald-700 transition-colors"
            type="button"
          >
            Export CSV
          </button>
        </div>
      </div>
      <div class="mt-4 flex gap-2 sm:hidden">
        <button
          on:click={resetAllTimers}
          class="flex-1 inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-orange-600 rounded-lg shadow-sm hover:bg-orange-700 transition-colors"
          type="button"
        >
          Reset All
        </button>
        <button
          on:click={exportTasksAsCsv}
          class="flex-1 inline-flex items-center justify-center px-3 py-2 text-sm font-semibold text-white bg-emerald-600 rounded-lg shadow-sm hover:bg-emerald-700 transition-colors"
          type="button"
        >
          Export CSV
        </button>
      </div>
    </header>

    <form on:submit={addTask} class="flex flex-col sm:flex-row space-y-2 sm:space-y-0 sm:space-x-2 mb-8">
      <input
        type="text"
        bind:value={taskInput}
        placeholder="Enter new task name..."
        class="flex-1 p-3 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      <button
        type="submit"
        class="p-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700 transition duration-200 shadow-sm"
      >
        Add Task
      </button>
    </form>

    <div class="mb-6 p-4 bg-gradient-to-r from-blue-50 to-indigo-50 rounded-lg border-2 border-blue-200">
      <div class="text-center">
        <p class="text-sm font-semibold text-gray-600 mb-2">Total Time</p>
        <p class="text-4xl font-mono font-bold text-blue-700">{formatTime(totalTime)}</p>
      </div>
    </div>

    <div class="space-y-4">
      {#if tasks.length === 0}
        <p class="text-gray-500 text-center">
          No tasks added yet. Add one above to get started!
        </p>
      {:else}
        {#each tasks as task, index (task.id)}
          <div
            class="flex flex-col sm:flex-row items-start sm:items-center justify-between bg-gray-50 p-4 rounded-lg shadow-sm border border-gray-200 cursor-pointer hover:bg-gray-100 transition duration-200 {task.is_running ? 'ring-2 ring-green-400' : ''}"
            on:click={() => toggleTimer(task.id)}
            role="button"
            tabindex="0"
          >
            <div class="flex-1 mb-3 sm:mb-0">
              <span class="text-lg font-medium text-gray-900 break-words">{task.label}</span>
              <span class="text-3xl font-mono text-gray-700 block mt-1">
                {formatTime(getCurrentElapsedTime(task as any))}
              </span>
            </div>
            <div class="flex space-x-2 w-full sm:w-auto" on:click|stopPropagation>
              <button
                on:click={() => moveTaskUp(task.id)}
                disabled={index === 0}
                class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold {index === 0 ? 'opacity-50 cursor-not-allowed' : ''}"
                title="Move up"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                </svg>
              </button>
              <button
                on:click={() => moveTaskDown(task.id)}
                disabled={index === tasks.length - 1}
                class="p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold {index === tasks.length - 1 ? 'opacity-50 cursor-not-allowed' : ''}"
                title="Move down"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>
              <button
                on:click={() => resetTimer(task.id)}
                class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-orange-500 hover:bg-orange-600 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Reset"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </button>
              <button
                on:click={() => editTaskTime(task.id)}
                class="w-1/3 sm:w-24 p-2 rounded-lg text-gray-800 bg-white border border-gray-300 hover:bg-gray-50 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Edit time"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
              </button>
              <button
                on:click={() => deleteTask(task.id)}
                class="w-1/3 sm:w-20 p-2 rounded-lg text-white bg-gray-400 hover:bg-gray-500 transition duration-200 flex items-center justify-center text-sm font-semibold"
                title="Delete"
              >
                <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
```

**Key Conversion Notes:**

1. **Reactive State**: Svelte's reactivity (`$:`) automatically updates the UI when `tasks` or `totalTime` change
2. **Event Handlers**: Replaced `addEventListener` with Svelte's `on:click`, `on:submit` directives
3. **Form Binding**: Used `bind:value` for two-way data binding on the input
4. **Template Rendering**: Replaced `innerHTML` manipulation with Svelte's `{#each}` blocks
5. **Real-time Updates**: Supabase real-time subscriptions work seamlessly with Svelte reactivity
6. **Timer Logic**: Preserved all timer functionality (start/stop, reset, edit time)
7. **CSV Export**: Maintained the same CSV export functionality
8. **8-Hour Notification**: Preserved the sound notification feature

#### 3.4.3 Add Tailwind CSS

Make sure Tailwind CSS is configured. If not already set up, install it:

```bash
cd apps/web
pnpm add -D tailwindcss postcss autoprefixer
npx tailwindcss init -p
```

Update `apps/web/tailwind.config.js`:

```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {},
  },
  plugins: [],
};
```

Create `apps/web/src/app.css`:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

Import it in `apps/web/src/routes/+layout.svelte`:

```svelte
<script>
  import '../app.css';
</script>

<slot />
```

---

## Phase 4: API Routes Implementation

### 4.1 Tasks API

Create `apps/web/src/routes/api/tasks/+server.ts`:

```typescript
import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { supabase } from '$lib/supabaseClient';
import { getSession } from '$lib/auth/helpers';

export const GET: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  const { data, error } = await supabase
    .from('tasks')
    .select('*')
    .eq('user_id', session.user.id)
    .order('position', { ascending: true });

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  return json({ tasks: data });
};

export const POST: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  const body = await request.json();
  const { data, error } = await supabase
    .from('tasks')
    .insert({
      user_id: session.user.id,
      ...body
    })
    .select()
    .single();

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  return json({ task: data });
};
```

### 4.2 Share Links API

Create `apps/web/src/routes/api/share/+server.ts`:

```typescript
import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { supabase } from '$lib/supabaseClient';
import { getSession } from '$lib/auth/helpers';

export const POST: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  // Generate unique token
  const shareToken = Buffer.from(crypto.getRandomValues(new Uint8Array(32)))
    .toString('base64')
    .replace(/[+/=]/g, '');

  const { data, error } = await supabase
    .from('view_links')
    .insert({
      user_id: session.user.id,
      share_token: shareToken,
      is_active: true
    })
    .select()
    .single();

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  const shareUrl = `${request.url.split('/api')[0]}/view/${shareToken}`;

  return json({ shareUrl, token: shareToken });
};
```

### 4.3 Webhooks API

Create `apps/web/src/routes/api/webhooks/+server.ts`:

```typescript
import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { supabaseAdmin } from '$lib/supabaseServer';
import { encryptWebhookUrl, decryptWebhookUrl } from '$lib/encryption/webhook';
import { getSession } from '$lib/auth/helpers';

export const GET: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  const { data, error } = await supabaseAdmin
    .from('webhooks')
    .select('id, name, description, is_active, last_triggered_at, created_at')
    .eq('user_id', session.user.id);

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  return json({ webhooks: data });
};

export const POST: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  const { url, name, description } = await request.json();

  // Encrypt webhook URL before storing
  const encryptedUrl = encryptWebhookUrl(url);

  const { data, error } = await supabaseAdmin
    .from('webhooks')
    .insert({
      user_id: session.user.id,
      url_encrypted: encryptedUrl,
      name,
      description,
      is_active: true
    })
    .select()
    .single();

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  return json({ webhook: { ...data, url: undefined } }); // Don't return URL
};
```

### 4.4 Webhook Sender

Create `apps/web/src/routes/api/webhooks/send/+server.ts`:

```typescript
import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { supabaseAdmin } from '$lib/supabaseServer';
import { decryptWebhookUrl } from '$lib/encryption/webhook';
import { getSession } from '$lib/auth/helpers';

export const POST: RequestHandler = async ({ request }) => {
  const session = await getSession();
  if (!session) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  const { taskData } = await request.json();

  // Get user's active webhooks
  const { data: webhooks, error } = await supabaseAdmin
    .from('webhooks')
    .select('*')
    .eq('user_id', session.user.id)
    .eq('is_active', true);

  if (error) {
    return json({ error: error.message }, { status: 500 });
  }

  // Send to all webhooks
  const results = await Promise.allSettled(
    webhooks.map(async (webhook) => {
      try {
        const decryptedUrl = decryptWebhookUrl(webhook.url_encrypted);
        
        const response = await fetch(decryptedUrl, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            text: `Task Timer Update: ${taskData.label} - ${formatTime(taskData.elapsedTime)}`
          })
        });

        // Update last_triggered_at
        await supabaseAdmin
          .from('webhooks')
          .update({ last_triggered_at: new Date().toISOString() })
          .eq('id', webhook.id);

        return { success: true, webhookId: webhook.id };
      } catch (err) {
        console.error(`Webhook ${webhook.id} failed:`, err);
        return { success: false, webhookId: webhook.id, error: err };
      }
    })
  );

  return json({ results });
};
```

---

## Phase 5: Real-Time View Mode

### 5.1 View Page Component

Create `apps/web/src/routes/view/[token]/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { supabase } from '$lib/supabaseClient';

  let tasks: any[] = [];
  let totalTime = 0;
  let loading = true;
  let error: string | null = null;
  let channel: any = null;

  $: token = $page.params.token;

  onMount(async () => {
    await loadTasks();

    // Subscribe to real-time updates
    channel = supabase
      .channel(`view-${token}`)
      .on(
        'postgres_changes',
        {
          event: '*',
          schema: 'public',
          table: 'tasks'
        },
        (payload: any) => {
          handleRealtimeUpdate(payload);
        }
      )
      .subscribe();

    // Update total time every second
    const interval = setInterval(updateTotalTime, 1000);
    
    return () => {
      clearInterval(interval);
    };
  });

  onDestroy(() => {
    if (channel) {
      supabase.removeChannel(channel);
    }
  });

  async function loadTasks() {
    try {
      const { data, error: dbError } = await supabase
        .rpc('get_tasks_for_view', { token });

      if (dbError) throw dbError;

      tasks = data || [];
      updateTotalTime();
      loading = false;
    } catch (err: any) {
      error = err.message || 'Failed to load tasks';
      loading = false;
    }
  }

  function handleRealtimeUpdate(payload: any) {
    if (payload.eventType === 'INSERT') {
      tasks.push(payload.new);
    } else if (payload.eventType === 'UPDATE') {
      const index = tasks.findIndex(t => t.id === payload.new.id);
      if (index !== -1) {
        tasks[index] = payload.new;
      }
    } else if (payload.eventType === 'DELETE') {
      tasks = tasks.filter(t => t.id !== payload.old.id);
    }
    updateTotalTime();
  }

  function updateTotalTime() {
    totalTime = tasks.reduce((sum, task) => {
      if (task.is_running && task.start_time) {
        const elapsed = Math.floor(
          (Date.now() - new Date(task.start_time).getTime()) / 1000
        );
        return sum + task.elapsed_time + elapsed;
      }
      return sum + task.elapsed_time;
    }, 0);
  }

  // formatTime is imported from shared package
</script>

{#if loading}
  <div>Loading...</div>
{:else if error}
  <div class="error">{error}</div>
{:else}
  <div class="view-container">
    <h1>Task Timer - View Mode</h1>
    <div class="total-time">
      Total: {formatTime(totalTime)}
    </div>
    <div class="tasks">
      {#each tasks as task}
        <div class="task-card">
          <h3>{task.label}</h3>
          <div class="time">
            {formatTime(
              task.is_running && task.start_time
                ? task.elapsed_time + Math.floor(
                    (Date.now() - new Date(task.start_time).getTime()) / 1000
                  )
                : task.elapsed_time
            )}
          </div>
          {#if task.is_running}
            <span class="status">Running</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
```

---

## Phase 6: Tauri Desktop App Sync

### 6.1 Update Tauri App to Sync with Supabase

Modify `apps/desktop/src-tauri/src/lib.rs` to add Supabase sync:

```rust
// Add HTTP client dependency to Cargo.toml
// reqwest = { version = "0.11", features = ["json"] }

#[tauri::command]
async fn sync_tasks_to_cloud(
    tasks: Vec<Task>,
    supabase_url: String,
    supabase_key: String,
    user_token: String,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    
    // Sync tasks to Supabase
    let response = client
        .post(&format!("{}/rest/v1/tasks", supabase_url))
        .header("apikey", &supabase_key)
        .header("Authorization", &format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .json(&tasks)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    if !response.status().is_success() {
        return Err("Failed to sync tasks".to_string());
    }
    
    Ok(())
}
```

---

## Phase 7: Deployment

### 7.1 Vercel Configuration

Create `apps/web/vercel.json`:

```json
{
  "buildCommand": "npm run build",
  "outputDirectory": ".svelte-kit",
  "framework": "sveltekit",
  "env": {
    "PUBLIC_SUPABASE_URL": "@supabase-url",
    "PUBLIC_SUPABASE_PUBLISHABLE_KEY": "@supabase-publishable-key"
  }
}
```

**Note**: If using legacy keys, replace `PUBLIC_SUPABASE_PUBLISHABLE_KEY` with `PUBLIC_SUPABASE_ANON_KEY`.

### 7.2 Environment Variables in Vercel

Set these in Vercel dashboard:
- `PUBLIC_SUPABASE_URL` - Your Supabase project URL
- `PUBLIC_SUPABASE_PUBLISHABLE_KEY` - Your publishable key (or `PUBLIC_SUPABASE_ANON_KEY` if using legacy keys)
- `SUPABASE_SERVICE_ROLE_KEY` (private) - Service role key - never expose to client
- `ENCRYPTION_KEY` (private) - 32-byte key for webhook encryption (generate using the commands in section 1.7)

---

## Phase 8: Testing Checklist

### 8.1 Authentication
- [ ] User registration
- [ ] User login
- [ ] Session persistence
- [ ] Logout

### 8.2 Task Management
- [ ] Create task
- [ ] Update task
- [ ] Delete task
- [ ] Reorder tasks
- [ ] Start/stop timer
- [ ] Real-time sync across devices

### 8.3 View Mode
- [ ] Generate shareable link
- [ ] View tasks via shareable link
- [ ] Real-time updates in view mode
- [ ] Token expiration

### 8.4 Webhooks
- [ ] Add webhook (encrypted storage)
- [ ] List webhooks
- [ ] Trigger webhook
- [ ] Webhook security (URL not exposed)

### 8.5 Desktop Sync
- [ ] Tauri app syncs with cloud
- [ ] Offline mode with sync on reconnect

---

## Timeline Estimate

- **Phase 1**: 1-2 days (Setup)
- **Phase 2**: 1 day (Database)
- **Phase 3**: 3-5 days (Frontend migration)
- **Phase 4**: 2-3 days (API routes)
- **Phase 5**: 1-2 days (View mode)
- **Phase 6**: 2-3 days (Tauri sync)
- **Phase 7**: 1 day (Deployment)
- **Phase 8**: 2-3 days (Testing)

**Total**: ~2-3 weeks for full migration

---

## Security Considerations

1. **Webhook URLs**: Always encrypted at rest, never exposed to frontend
2. **RLS Policies**: Strict row-level security on all tables
3. **Authentication**: Supabase handles JWT tokens securely
4. **API Keys**: Service role key only used server-side
5. **Share Tokens**: Long, random, base64-encoded tokens
6. **HTTPS**: All communications encrypted in transit

---

## Migration Notes

- Keep existing Tauri app functional during migration
- Use feature flags to toggle between old/new implementations
- Migrate users gradually (can support both systems initially)
- Export existing localStorage data for users migrating
- Desktop app can use shared package for common utilities
- Both apps can sync with the same Supabase backend

## Quick Start Commands

After monorepo setup:

```bash
# Install all dependencies
pnpm install

# Run desktop app
pnpm dev:desktop

# Run web app
pnpm dev:web

# Build both
pnpm build:all
```

---

## Resources

- [SvelteKit Documentation](https://kit.svelte.dev/)
- [Supabase Documentation](https://supabase.com/docs)
- [Supabase Real-time](https://supabase.com/docs/guides/realtime)
- [Vercel Deployment](https://vercel.com/docs)