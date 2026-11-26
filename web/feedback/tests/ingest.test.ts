import { beforeEach, describe, expect, test } from 'vitest';
import type { Db } from '@/lib/db';
import type { FileStore } from '@/lib/files';
import { handleUpload } from '@/lib/ingest';
import { sweep } from '@/lib/limits';
import { getReport, listReports } from '@/lib/store';
import { fields, memoryStore, png, save, testDb } from './helpers';

const ADDRESS = '203.0.113.7';
let db: Db;
let held: Map<string, Uint8Array>;
let files: FileStore;
let clock: Date;
beforeEach(async () => {
  db = await testDb();
  ({ store: files, held } = memoryStore());
  clock = new Date('2026-11-26T12:00:00Z');
});

const id = (n: number) => n.toString(16).padStart(32, '0');
type Parts = Partial<Record<'screenshot' | 'log' | 'save' | 'extra', Uint8Array>>;
const all: Parts = { screenshot: png(640, 360), log: new TextEncoder().encode('açıldı\n'), save: save() };

function request(n: number, parts: Parts = {}, headers: Record<string, string> = {}, over: object = {}): Request {
  const form = new FormData();
  form.set('report', JSON.stringify({ ...fields, id: id(n), ...over }));
  for (const [name, bytes] of Object.entries(parts)) form.set(name, new Blob([bytes as BlobPart]), '../../evil.exe');
  return new Request('https://example.test/api/reports', { method: 'POST', body: form, headers: { 'x-real-ip': ADDRESS, ...headers } });
}
const send = (req: Request, deps: { db?: Db; files?: FileStore } = {}) =>
  handleUpload(req, { db: deps.db ?? db, files: deps.files ?? files, secret: 's'.repeat(32), now: () => clock });
const later = (ms: number) => (clock = new Date(clock.getTime() + ms));
const HOUR = 3_600_000;

async function earlier(count: number, agoMs: number, hash = 'someone') {
  for (let i = 0; i < count; i++) {
    await db.query('INSERT INTO uploads (token, ip_hash, at) VALUES ($1, $2, $3)', [`seed-${agoMs}-${i}`, hash, new Date(clock.getTime() - agoMs).toISOString()]);
  }
}

describe('a good upload', () => {
  test('answers 201 and stores the row and the three files', async () => {
    const res = await send(request(1, all));
    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ id: id(1) });
    const row = (await getReport(db, id(1)))!;
    expect(Object.keys(row.files).sort()).toEqual(['log', 'save', 'screenshot']);
    expect(row.files.screenshot).toMatch(new RegExp(`^reports/${id(1)}/screenshot\\.png`));
    expect(row.files.log).toMatch(/log\.txt/);
    expect(row.files.save).toMatch(/save\.alz/);
    expect(held.get(row.files.log!)).toEqual(all.log);
    expect(row.bytes).toBe(all.screenshot!.length + all.log!.length + all.save!.length);
  });
  test('needs no files, and an unknown part is ignored', async () => {
    expect((await send(request(1, { extra: save() }))).status).toBe(201);
    expect((await getReport(db, id(1)))!.files).toEqual({});
    expect(held.size).toBe(0);
  });
  test('the same id again answers 200 and adds nothing', async () => {
    await send(request(1, all));
    for (let i = 0; i < 8; i++) expect((await send(request(1, all))).status).toBe(200);
    expect(await listReports(db, {})).toHaveLength(1);
    expect(held.size).toBe(3);
  });
});

describe('a refused upload', () => {
  test.each([
    ['bad fields', () => request(1, {}, {}, { kind: 'x' })],
    ['a screenshot that is not a PNG', () => request(1, { screenshot: save() })],
    ['a body that is not a form', () => new Request('https://example.test/api/reports', { method: 'POST', body: '{}', headers: { 'content-type': 'application/json' } })],
    ['a broken form', () => new Request('https://example.test/api/reports', { method: 'POST', body: 'nonsense', headers: { 'content-type': 'multipart/form-data; boundary=x' } })],
  ])('%s answers 400', async (_, make) => {
    expect((await send(make())).status).toBe(400);
    expect(held.size).toBe(0);
    expect(await db.query('SELECT 1 FROM uploads')).toHaveLength(0);
  });
  test('a text part where a file belongs answers 400', async () => {
    const form = new FormData();
    form.set('report', JSON.stringify(fields));
    form.set('log', 'text');
    expect((await send(new Request('https://example.test/api/reports', { method: 'POST', body: form }))).status).toBe(400);
  });
  test('a declared size over the limit answers 413', async () => {
    expect((await send(request(1, {}, { 'content-length': '4000001' }))).status).toBe(413);
  });
  test('a file over its size answers 413', async () => {
    expect((await send(request(1, { screenshot: png(10, 10, 2_500_001) }))).status).toBe(413);
    expect(held.size).toBe(0);
  });
});

describe('limits', () => {
  test('the sixth report of one address in an hour waits', async () => {
    for (let n = 1; n <= 5; n++) {
      expect((await send(request(n))).status).toBe(201);
      later(60_000);
    }
    const res = await send(request(6, all));
    expect(res.status).toBe(429);
    // the first report leaves the window 55 minutes from now
    expect(Number(res.headers.get('retry-after'))).toBe(55 * 60);
    expect(held.size).toBe(0);
    expect((await send(request(7, {}, { 'x-real-ip': '198.51.100.1' }))).status).toBe(201);
    later(55 * 60_000 + 1000);
    expect((await send(request(6))).status).toBe(201);
  });
  test('the platform address header wins over the other one', async () => {
    for (let n = 1; n <= 5; n++) await send(request(n, {}, { 'x-vercel-forwarded-for': '198.51.100.2, 10.0.0.1', 'x-real-ip': `10.0.0.${n}` }));
    expect((await send(request(6, {}, { 'x-vercel-forwarded-for': '198.51.100.2' }))).status).toBe(429);
  });
  test('the twenty-first report of one address in a day waits', async () => {
    for (let n = 1; n <= 20; n++) {
      expect((await send(request(n))).status).toBe(201);
      later(HOUR / 4);
    }
    expect((await send(request(21))).status).toBe(429);
  });
  test('all senders together stop at 100 a day', async () => {
    await earlier(99, 2 * HOUR);
    expect((await send(request(1))).status).toBe(201);
    expect((await send(request(2))).status).toBe(503);
  });
  test('all senders together stop at 500 in 30 days', async () => {
    await earlier(499, 10 * 24 * HOUR);
    expect((await send(request(1))).status).toBe(201);
    expect((await send(request(2))).status).toBe(503);
  });
  test('a full store takes nothing', async () => {
    await db.query(`INSERT INTO reports (id, created_at, kind, version, fields, files, bytes) VALUES ('x', now(), 'bug', '1', '{}', '{}', 800000000)`);
    expect((await send(request(1, all))).status).toBe(503);
    expect(held.size).toBe(0);
  });
  test('twenty at once from one address let exactly five in', async () => {
    const answers = await Promise.all(Array.from({ length: 20 }, (_, i) => send(request(i + 1, all))));
    expect(answers.filter((r) => r.status === 201)).toHaveLength(5);
    expect(answers.filter((r) => r.status === 429)).toHaveLength(15);
    expect(await listReports(db, {})).toHaveLength(5);
    expect(held.size).toBe(15);
  });
  test('the same id twice at once keeps one report and its files only', async () => {
    const answers = await Promise.all([send(request(1, all)), send(request(1, all))]);
    expect(answers.map((r) => r.status).sort()).toEqual([200, 201]);
    expect(held.size).toBe(3);
    for (const path of Object.values((await getReport(db, id(1)))!.files)) expect(held.has(path)).toBe(true);
  });
});

describe('failures', () => {
  test('a store that fails midway leaves nothing', async () => {
    let calls = 0;
    const broken: FileStore = { ...files, put: async (...args) => (++calls === 2 ? Promise.reject(new Error('down')) : files.put(...args)) };
    expect((await send(request(1, all), { files: broken })).status).toBe(503);
    expect(held.size).toBe(0);
    expect(await listReports(db, {})).toHaveLength(0);
  });
  test('a database that fails on the report leaves no files', async () => {
    const broken: Db = { query: async (sql, params) => (sql.includes('INSERT INTO reports') ? Promise.reject(new Error('down')) : db.query(sql, params)) };
    expect((await send(request(1, all), { db: broken })).status).toBe(503);
    expect(held.size).toBe(0);
  });
  test('a database that is down answers 503', async () => {
    const down: Db = { query: async () => Promise.reject(new Error('down')) };
    expect((await send(request(1, all), { db: down })).status).toBe(503);
    expect(held.size).toBe(0);
  });
});

describe('addresses', () => {
  test('no row holds the address', async () => {
    await send(request(1));
    const rows = await db.query('SELECT * FROM uploads');
    expect(rows).toHaveLength(1);
    expect(JSON.stringify(rows)).not.toContain(ADDRESS);
  });
  test('sweep forgets the hash after two days and the row after 31', async () => {
    await earlier(1, 3 * 24 * HOUR);
    await earlier(1, 32 * 24 * HOUR);
    await earlier(1, HOUR);
    await sweep(db, clock);
    const rows = await db.query<{ ip_hash: string | null }>('SELECT ip_hash FROM uploads ORDER BY at');
    expect(rows.map((r) => r.ip_hash)).toEqual([null, 'someone']);
  });
});
