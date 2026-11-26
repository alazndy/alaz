import { randomBytes } from 'node:crypto';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { neon } from '@neondatabase/serverless';
import { del, get, put } from '@vercel/blob';
import type { Db } from './db';
import type { FileStore } from './files';

export type Settings =
  | { kind: 'cloud'; databaseUrl: string; secret: string; ownerEmail?: string }
  | { kind: 'local'; dir: string; secret: string; ownerEmail?: string };

/** Null when something required is missing. A local setting on a real server is refused, never ignored. */
export function settings(env: Record<string, string | undefined>): Settings | null {
  const secret = env.IP_HASH_SECRET ?? '';
  if (secret.length < 32) return null;
  const ownerEmail = env.ADMIN_EMAIL || undefined;
  if (env.FEEDBACK_LOCAL_DIR) {
    if (env.NODE_ENV === 'production' || env.VERCEL) return null;
    return { kind: 'local', dir: env.FEEDBACK_LOCAL_DIR, secret, ownerEmail };
  }
  return env.DATABASE_URL ? { kind: 'cloud', databaseUrl: env.DATABASE_URL, secret, ownerEmail } : null;
}

export function neonDb(url: string): Db {
  const sql = neon(url);
  return { query: async <T,>(text: string, params?: unknown[]) => (await sql.query(text, params)) as T[] };
}

/** A private Blob store: files are read only through this server. */
export function blobStore(): FileStore {
  return {
    put: async (name, bytes, contentType) =>
      (await put(name, Buffer.from(bytes), { access: 'private', contentType, addRandomSuffix: true })).pathname,
    get: async (name) => {
      const found = await get(name, { access: 'private' });
      return found?.statusCode === 200 ? new Uint8Array(await new Response(found.stream).arrayBuffer()) : null;
    },
    del: async (names) => {
      if (names.length) await del(names);
    },
  };
}

/** Development only: Postgres inside this process, kept on disk. */
export async function localDb(dir: string): Promise<Db> {
  const { PGlite } = await import('@electric-sql/pglite');
  await mkdir(dir, { recursive: true });
  const pg = new PGlite(path.join(dir, 'db'));
  return { query: async <T,>(text: string, params?: unknown[]) => (await pg.query<T>(text, params)).rows };
}

export function diskStore(root: string): FileStore {
  const base = path.resolve(root);
  const inside = (name: string) => {
    const full = path.resolve(base, name);
    if (!full.startsWith(base + path.sep)) throw new Error('path outside the store');
    return full;
  };
  return {
    put: async (name, bytes) => {
      inside(name);
      const stored = `${name}-${randomBytes(8).toString('hex')}`;
      const full = inside(stored);
      await mkdir(path.dirname(full), { recursive: true });
      await writeFile(full, bytes);
      return stored;
    },
    get: async (name) => {
      const full = inside(name);
      try {
        return new Uint8Array(await readFile(full));
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null;
        throw error;
      }
    },
    del: async (names) => {
      for (const full of names.map(inside)) await rm(full, { force: true });
    },
  };
}
