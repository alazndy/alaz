import path from 'node:path';
import { blobStore, diskStore, localDb, neonDb, settings, type Settings } from './adapters';
import { ensureSchema, type Db } from './db';
import type { FileStore } from './files';

export type Deps = { db: Db; files: FileStore; secret: string; ownerEmail?: string };

async function connect(found: Settings): Promise<Deps> {
  const { secret, ownerEmail } = found;
  const local = found.kind === 'local';
  const db = local ? await localDb(found.dir) : neonDb(found.databaseUrl);
  const files = local ? diskStore(path.join(found.dir, 'files')) : blobStore();
  await ensureSchema(db);
  return { db, files, secret, ownerEmail };
}

// kept on globalThis so a development reload does not open the local database a second time
const cache = globalThis as { feedbackDeps?: Promise<Deps> };

/** Null when a required setting is missing. A failed start is tried again on the next request. */
export async function deps(): Promise<Deps | null> {
  const found = settings(process.env);
  if (!found) return null;
  cache.feedbackDeps ??= connect(found).catch((error) => {
    cache.feedbackDeps = undefined;
    throw error;
  });
  return cache.feedbackDeps;
}
