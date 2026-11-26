import { z } from 'zod';
import { canManage, type Viewer } from './access';
import type { Db } from './db';
import type { FileStore } from './files';
import { STATUSES } from './report';
import { addMember, deleteReport, getReport, removeMember, setStatus } from './store';

export class Forbidden extends Error {}
export class Invalid extends Error {}

type Deps = { db: Db; files: FileStore };

function mustManage(viewer: Viewer | null): void {
  if (!canManage(viewer)) throw new Forbidden('only the owner may do this');
}

function parse<T>(schema: z.ZodType<T>, value: unknown): T {
  const result = schema.safeParse(value);
  if (!result.success) throw new Invalid('value not accepted');
  return result.data;
}

const status = z.enum(STATUSES);
const address = z.string().trim().max(254).pipe(z.email());

export async function changeStatus(deps: Deps, viewer: Viewer | null, id: string, value: string): Promise<void> {
  mustManage(viewer);
  await setStatus(deps.db, id, parse(status, value));
}

/** Files first: if they cannot be deleted the row stays, so nothing is left behind without a trace. */
export async function removeReport(deps: Deps, viewer: Viewer | null, id: string): Promise<void> {
  mustManage(viewer);
  const report = await getReport(deps.db, id);
  if (!report) return;
  await deps.files.del(Object.values(report.files));
  await deleteReport(deps.db, id);
}

export async function invite(deps: Deps, viewer: Viewer | null, email: string): Promise<void> {
  mustManage(viewer);
  await addMember(deps.db, parse(address, email));
}

export async function uninvite(deps: Deps, viewer: Viewer | null, email: string): Promise<void> {
  mustManage(viewer);
  await removeMember(deps.db, email);
}
