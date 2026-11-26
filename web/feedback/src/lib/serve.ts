import { canRead, type Viewer } from './access';
import type { Db } from './db';
import type { FileStore } from './files';
import { FILE_NAMES, FILE_TYPES, type FileName } from './report';
import { getReport } from './store';

const isFileName = (name: string): name is FileName => (FILE_NAMES as readonly string[]).includes(name);
const empty = (status: number) => new Response(null, { status, headers: { 'Cache-Control': 'private, no-store' } });

export async function serveFile(deps: { db: Db; files: FileStore }, viewer: Viewer | null, id: string, name: string): Promise<Response> {
  if (!viewer) return empty(401);
  if (!canRead(viewer)) return empty(403);
  if (!isFileName(name)) return empty(404);
  const path = (await getReport(deps.db, id))?.files[name];
  const bytes = path ? await deps.files.get(path) : null;
  if (!bytes) return empty(404);
  const { ext, type } = FILE_TYPES[name];
  return new Response(bytes as BodyInit, {
    headers: {
      'Content-Type': type,
      'Content-Disposition': name === 'screenshot' ? 'inline' : `attachment; filename="${id}.${ext}"`,
      'Cache-Control': 'private, no-store',
      'X-Content-Type-Options': 'nosniff',
      // the file is a stranger's: nothing in it may run on this site
      'Content-Security-Policy': "default-src 'none'; sandbox",
    },
  });
}
