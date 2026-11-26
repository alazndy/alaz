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
];

export async function ensureSchema(db: Db): Promise<void> {
  for (const statement of SCHEMA) await db.query(statement);
}
