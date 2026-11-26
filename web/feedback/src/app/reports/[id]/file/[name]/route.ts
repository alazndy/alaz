import { deps } from '@/lib/deps';
import { serveFile } from '@/lib/serve';
import { currentViewer } from '@/lib/session';

export async function GET(_req: Request, { params }: { params: Promise<{ id: string; name: string }> }) {
  const d = await deps();
  if (!d) return new Response(null, { status: 503 });
  const { id, name } = await params;
  return serveFile(d, await currentViewer(d), id, name);
}
