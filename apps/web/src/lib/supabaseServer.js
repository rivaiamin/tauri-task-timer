import { createClient } from '@supabase/supabase-js';
import { PUBLIC_SUPABASE_URL } from '$env/static/public';
import { SUPABASE_SERVICE_ROLE_KEY } from '$env/static/private';

// Server-side client with the service role (bypasses Row Level Security).
// Constructed lazily on first use so that merely importing this module does not
// throw when env vars are absent (e.g. during the build/analyse step or CI).
/** @type {import('@supabase/supabase-js').SupabaseClient | undefined} */
let _client;
function getClient() {
  if (!_client) {
    _client = createClient(PUBLIC_SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY, {
      auth: {
        autoRefreshToken: false,
        persistSession: false
      }
    });
  }
  return _client;
}

// Transparent lazy proxy: `supabaseAdmin.from(...)` works exactly like a real
// client, but the underlying client is only created on first property access.
export const supabaseAdmin = /** @type {import('@supabase/supabase-js').SupabaseClient} */ (
  new Proxy(
    {},
    {
      get(_target, prop) {
        const client = /** @type {any} */ (getClient());
        const value = client[prop];
        return typeof value === 'function' ? value.bind(client) : value;
      }
    }
  )
);
