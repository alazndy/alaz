import { deps } from '@/lib/deps';
import { handleUpload } from '@/lib/ingest';

export async function POST(req: Request) {
  const d = await deps().catch(() => null);
  if (!d) return Response.json({ error: 'unavailable' }, { status: 503 });
  return handleUpload(req, { db: d.db, files: d.files, secret: d.secret, now: () => new Date() });
}
