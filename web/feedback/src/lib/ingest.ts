import { createHmac } from 'node:crypto';
import type { Db } from './db';
import type { FileStore } from './files';
import { reserve, sweep } from './limits';
import { checkFile, FILE_NAMES, FILE_TYPES, parseFields, RATE, SIZE, type FileName } from './report';
import { getReport, insertReport, usage, type StoredFiles } from './store';

export type UploadDeps = { db: Db; files: FileStore; secret: string; now: () => Date };

const answer = (status: number, body: object, headers?: HeadersInit) => Response.json(body, { status, headers });
const invalid = () => answer(400, { error: 'invalid' });
const tooLarge = () => answer(413, { error: 'too_large' });
const full = () => answer(503, { error: 'full' });

// Vercel sets these headers itself, so a sender cannot choose the value there.
function address(req: Request): string {
  const header = req.headers.get('x-vercel-forwarded-for') ?? req.headers.get('x-real-ip') ?? '';
  return header.split(',')[0]!.trim() || 'unknown';
}

type Part = { name: FileName; bytes: Uint8Array };

export async function handleUpload(req: Request, deps: UploadDeps): Promise<Response> {
  if (!req.headers.get('content-type')?.toLowerCase().startsWith('multipart/form-data')) return invalid();
  if (Number(req.headers.get('content-length') ?? 0) > SIZE.body) return tooLarge();

  let form: FormData;
  try {
    form = await req.formData();
  } catch {
    return invalid();
  }
  const report = form.get('report');
  const fields = typeof report === 'string' ? parseFields(report) : null;
  if (!fields) return invalid();

  const parts: Part[] = [];
  let total = 0;
  for (const name of FILE_NAMES) {
    const value = form.get(name);
    if (value === null) continue;
    if (typeof value === 'string') return invalid();
    total += value.size;
    if (value.size > SIZE[name] || total > SIZE.body) return tooLarge();
    const bytes = new Uint8Array(await value.arrayBuffer());
    if (!checkFile(name, bytes)) return invalid();
    parts.push({ name, bytes });
  }

  const stored: StoredFiles = {};
  try {
    const now = deps.now();
    // a retry after a lost answer must not use up the sender's limit
    if (await getReport(deps.db, fields.id)) return answer(200, { id: fields.id });
    if ((await usage(deps.db, now)).bytes + total > RATE.storedBytes) return full();
    const ipHash = createHmac('sha256', deps.secret).update(address(req)).digest('hex');
    const place = await reserve(deps.db, ipHash, now);
    if (!place.ok) {
      if (place.status === 503) return full();
      return answer(429, { error: 'rate' }, { 'Retry-After': String(place.retryAfter) });
    }
    for (const { name, bytes } of parts) {
      const { ext, type } = FILE_TYPES[name];
      stored[name] = await deps.files.put(`reports/${fields.id}/${name}.${ext}`, bytes, type);
    }
    if (!(await insertReport(deps.db, fields, stored, total, now))) {
      // the same report arrived twice at once; the other one is stored
      await discard(deps.files, stored);
      return answer(200, { id: fields.id });
    }
    await sweep(deps.db, now).catch(() => {});
    return answer(201, { id: fields.id });
  } catch (error) {
    // the name only: a database message can carry report text
    console.error('upload failed', error instanceof Error ? error.name : 'unknown');
    await discard(deps.files, stored);
    return answer(503, { error: 'unavailable' });
  }
}

async function discard(files: FileStore, stored: StoredFiles): Promise<void> {
  const paths = Object.values(stored);
  if (paths.length) await files.del(paths).catch(() => console.error('upload cleanup failed'));
}
