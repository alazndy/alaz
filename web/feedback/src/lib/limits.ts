import { randomUUID } from 'node:crypto';
import type { Db } from './db';
import { RATE } from './report';

export type Reservation = { ok: true } | { ok: false; status: 429 | 503; retryAfter?: number };

/** Take one place under the upload limits, or say why not. The place counts even if the upload later fails. */
export async function reserve(db: Db, ipHash: string, now: Date): Promise<Reservation> {
  const [found] = await db.query<{ verdict: 'ok' | 'address' | 'full'; wait_until: string | Date | null }>(
    'SELECT verdict, wait_until FROM reserve_upload($1, $2, $3, $4, $5, $6, $7)',
    [randomUUID(), ipHash, now.toISOString(), RATE.addressHour, RATE.addressDay, RATE.allDay, RATE.all30d],
  );
  if (!found) throw new Error('no verdict');
  if (found.verdict === 'ok') return { ok: true };
  if (found.verdict === 'full') return { ok: false, status: 503 };
  const wait = found.wait_until ? new Date(found.wait_until).getTime() - now.getTime() : 0;
  return { ok: false, status: 429, retryAfter: Math.max(1, Math.ceil(wait / 1000)) };
}

/** Forget address hashes after two days, and rows after they stop counting. */
export async function sweep(db: Db, now: Date): Promise<void> {
  const ago = (days: number) => new Date(now.getTime() - days * 86_400_000).toISOString();
  await db.query('DELETE FROM uploads WHERE at < $1', [ago(31)]);
  await db.query('UPDATE uploads SET ip_hash = NULL WHERE ip_hash IS NOT NULL AND at < $1', [ago(2)]);
}
