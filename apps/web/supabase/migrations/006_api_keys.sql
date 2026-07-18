-- API keys for programmatic / AI-agent access to a user's tasks.
-- Only the SHA-256 hash of each key is stored; the raw key is shown once, at
-- creation, and is unrecoverable afterwards.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS api_keys (
  id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  user_id      UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  key_hash     TEXT NOT NULL UNIQUE,          -- sha256(raw_key), hex
  key_prefix   TEXT NOT NULL,                 -- first chars, for display only
  name         TEXT,
  scopes       TEXT[] NOT NULL DEFAULT ARRAY['tasks:read', 'tasks:write'],
  revoked      BOOLEAN NOT NULL DEFAULT FALSE,
  last_used_at TIMESTAMP WITH TIME ZONE,
  created_at   TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash) WHERE revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;

-- Users can list and revoke their own keys. The stored hash cannot be turned
-- back into a usable key, so exposing the row is safe. Minting happens via the
-- SECURITY DEFINER function below (or a server route), never a client insert.
CREATE POLICY "Users can view own api keys" ON api_keys
  FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can revoke own api keys" ON api_keys
  FOR UPDATE USING (auth.uid() = user_id);

-- Mint a new API key for a user (looked up by email). Returns the RAW key ONCE;
-- only its hash is persisted. Issue keys from the Supabase SQL editor, e.g.:
--   SELECT mint_api_key('you@example.com', 'claude-agent');
CREATE OR REPLACE FUNCTION mint_api_key(p_email TEXT, p_name TEXT DEFAULT NULL)
RETURNS TEXT AS $$
DECLARE
  target_user_id UUID;
  raw_key TEXT;
BEGIN
  SELECT id INTO target_user_id FROM auth.users WHERE email = p_email;
  IF target_user_id IS NULL THEN
    RAISE EXCEPTION 'No user with email %', p_email;
  END IF;

  raw_key := 'sk_live_' || encode(gen_random_bytes(24), 'hex');

  INSERT INTO api_keys (user_id, key_hash, key_prefix, name)
  VALUES (
    target_user_id,
    encode(digest(raw_key, 'sha256'), 'hex'),
    left(raw_key, 16),
    p_name
  );

  RETURN raw_key;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
