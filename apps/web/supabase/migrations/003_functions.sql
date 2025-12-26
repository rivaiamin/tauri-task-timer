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
  "position" INTEGER,
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