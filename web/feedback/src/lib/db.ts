export interface Db {
  query<T = Record<string, unknown>>(sql: string, params?: unknown[]): Promise<T[]>;
}

// one statement each: the Neon HTTP driver runs a single statement per call
const SCHEMA = [
  `CREATE TABLE IF NOT EXISTS reports (
     id text PRIMARY KEY,
     created_at timestamptz NOT NULL,
     status text NOT NULL DEFAULT 'new',
     kind text NOT NULL,
     version text NOT NULL,
     fields jsonb NOT NULL,
     files jsonb NOT NULL,
     bytes integer NOT NULL
   )`,
  `CREATE INDEX IF NOT EXISTS reports_newest ON reports (created_at DESC)`,
  `CREATE TABLE IF NOT EXISTS members (email text PRIMARY KEY)`,
  `CREATE TABLE IF NOT EXISTS uploads (token text PRIMARY KEY, ip_hash text, at timestamptz NOT NULL)`,
  `CREATE INDEX IF NOT EXISTS uploads_at ON uploads (at)`,
  // Check and reserve in one step. Inside the function each count sees what the previous holder of the lock wrote,
  // so callers at the same moment cannot pass a limit together. 'full' is a limit for all senders.
  `CREATE OR REPLACE FUNCTION reserve_upload(
     p_token text, p_ip text, p_now timestamptz, p_hour int, p_day int, p_all_day int, p_all_30 int
   ) RETURNS TABLE (verdict text, wait_until timestamptz) LANGUAGE plpgsql AS $fn$
   DECLARE
     seen int;
     oldest timestamptz;
   BEGIN
     PERFORM pg_advisory_xact_lock(7421001);
     SELECT COUNT(*), MIN(at) INTO seen, oldest FROM uploads WHERE ip_hash = p_ip AND at > p_now - interval '1 day';
     IF seen >= p_day THEN
       RETURN QUERY SELECT 'address'::text, oldest + interval '1 day';
       RETURN;
     END IF;
     SELECT COUNT(*), MIN(at) INTO seen, oldest FROM uploads WHERE ip_hash = p_ip AND at > p_now - interval '1 hour';
     IF seen >= p_hour THEN
       RETURN QUERY SELECT 'address'::text, oldest + interval '1 hour';
       RETURN;
     END IF;
     IF (SELECT COUNT(*) FROM uploads WHERE at > p_now - interval '1 day') >= p_all_day
        OR (SELECT COUNT(*) FROM uploads WHERE at > p_now - interval '30 days') >= p_all_30 THEN
       RETURN QUERY SELECT 'full'::text, NULL::timestamptz;
       RETURN;
     END IF;
     INSERT INTO uploads (token, ip_hash, at) VALUES (p_token, p_ip, p_now);
     RETURN QUERY SELECT 'ok'::text, NULL::timestamptz;
   END
   $fn$`,
];

export async function ensureSchema(db: Db): Promise<void> {
  for (const statement of SCHEMA) await db.query(statement);
}
