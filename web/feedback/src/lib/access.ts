import type { Db } from './db';
import { isMember } from './store';

export type Role = 'owner' | 'tester' | 'none';
export type Viewer = { email: string; role: Role };

/** The signed-in person's rights, read from the list on every request. `email` must already be verified. */
export async function viewerFor(db: Db, email: string | null, ownerEmail: string | undefined): Promise<Viewer | null> {
  const who = email?.trim().toLowerCase();
  if (!who) return null;
  if (who === ownerEmail?.trim().toLowerCase()) return { email: who, role: 'owner' };
  return { email: who, role: (await isMember(db, who)) ? 'tester' : 'none' };
}

export const canRead = (v: Viewer | null): boolean => v?.role === 'owner' || v?.role === 'tester';
export const canManage = (v: Viewer | null): boolean => v?.role === 'owner';
