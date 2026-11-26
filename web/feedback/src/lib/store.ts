import type { Db } from './db';
import type { Fields, FileName, Kind, Status } from './report';

export type StoredFiles = Partial<Record<FileName, string>>;
export type ReportRow = Fields & { status: Status; created_at: Date; files: StoredFiles; bytes: number };
export type Filter = { kind?: Kind; status?: Status; version?: string };

export const LIST_LIMIT = 200;
const DAY = 86_400_000;

type Raw = { fields: Fields; status: Status; created_at: string | Date; files: StoredFiles; bytes: number };
const COLUMNS = 'fields, status, created_at, files, bytes';
const row = (r: Raw): ReportRow => ({ ...r.fields, status: r.status, created_at: new Date(r.created_at), files: r.files, bytes: r.bytes });
const email = (value: string) => value.trim().toLowerCase();

/** False when a report with this id is already stored. */
export async function insertReport(db: Db, fields: Fields, files: StoredFiles, bytes: number, now: Date): Promise<boolean> {
  const added = await db.query(
    `INSERT INTO reports (id, created_at, kind, version, fields, files, bytes)
     VALUES ($1, $2, $3, $4, $5::jsonb, $6::jsonb, $7) ON CONFLICT (id) DO NOTHING RETURNING id`,
    [fields.id, now.toISOString(), fields.kind, fields.game_version, JSON.stringify(fields), JSON.stringify(files), bytes],
  );
  return added.length === 1;
}

export async function getReport(db: Db, id: string): Promise<ReportRow | null> {
  const found = await db.query<Raw>(`SELECT ${COLUMNS} FROM reports WHERE id = $1`, [id]);
  return found[0] ? row(found[0]) : null;
}

export async function listReports(db: Db, filter: Filter): Promise<ReportRow[]> {
  const found = await db.query<Raw>(
    `SELECT ${COLUMNS} FROM reports
     WHERE ($1::text IS NULL OR kind = $1) AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR version = $3)
     ORDER BY created_at DESC, id LIMIT ${LIST_LIMIT}`,
    [filter.kind ?? null, filter.status ?? null, filter.version ?? null],
  );
  return found.map(row);
}

export async function versions(db: Db): Promise<string[]> {
  const found = await db.query<{ version: string }>('SELECT DISTINCT version FROM reports ORDER BY version DESC');
  return found.map((r) => r.version);
}

export async function setStatus(db: Db, id: string, status: Status): Promise<void> {
  await db.query('UPDATE reports SET status = $2 WHERE id = $1', [id, status]);
}

/** Remove the row; the caller gets the paths of the files it pointed to. */
export async function deleteReport(db: Db, id: string): Promise<string[]> {
  const gone = await db.query<{ files: StoredFiles }>('DELETE FROM reports WHERE id = $1 RETURNING files', [id]);
  return gone[0] ? Object.values(gone[0].files) : [];
}

export async function usage(db: Db, now: Date): Promise<{ bytes: number; last30Days: number }> {
  const since = new Date(now.getTime() - 30 * DAY).toISOString();
  const [found] = await db.query<{ bytes: string | number; uploads: string | number }>(
    `SELECT (SELECT COALESCE(SUM(bytes), 0) FROM reports) AS bytes,
            (SELECT COUNT(*) FROM uploads WHERE at > $1) AS uploads`,
    [since],
  );
  return { bytes: Number(found!.bytes), last30Days: Number(found!.uploads) };
}

export async function listMembers(db: Db): Promise<string[]> {
  return (await db.query<{ email: string }>('SELECT email FROM members ORDER BY email')).map((r) => r.email);
}

export async function addMember(db: Db, value: string): Promise<void> {
  await db.query('INSERT INTO members (email) VALUES ($1) ON CONFLICT DO NOTHING', [email(value)]);
}

export async function removeMember(db: Db, value: string): Promise<void> {
  await db.query('DELETE FROM members WHERE email = $1', [email(value)]);
}

export async function isMember(db: Db, value: string): Promise<boolean> {
  return (await db.query('SELECT 1 FROM members WHERE email = $1', [email(value)])).length === 1;
}
