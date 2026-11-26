import { beforeEach, describe, expect, test } from 'vitest';
import type { Db } from '@/lib/db';
import { parseFields, type Fields } from '@/lib/report';
import { addMember, deleteReport, getReport, insertReport, isMember, listMembers, listReports, removeMember, setStatus, usage, versions } from '@/lib/store';
import { fields, testDb } from './helpers';

let db: Db;
beforeEach(async () => {
  db = await testDb();
});

const report = (n: number, over: object = {}): Fields => parseFields(JSON.stringify({ ...fields, id: String(n).padStart(32, '0'), ...over }))!;
const at = (iso: string) => new Date(iso);

describe('reports', () => {
  test('insert then get returns fields, files, size and status new', async () => {
    const r = report(1, { name: 'Ece', omitted: ['save'] });
    expect(await insertReport(db, r, { log: 'reports/1/log.txt' }, 42, at('2026-11-26T10:00:00Z'))).toBe(true);
    expect(await getReport(db, r.id)).toEqual({ ...r, status: 'new', created_at: at('2026-11-26T10:00:00Z'), files: { log: 'reports/1/log.txt' }, bytes: 42 });
    expect(await getReport(db, 'f'.repeat(32))).toBeNull();
  });

  test('a second insert with the same id is refused and keeps the first', async () => {
    await insertReport(db, report(1, { text: 'first' }), {}, 1, new Date());
    expect(await insertReport(db, report(1, { text: 'second' }), {}, 1, new Date())).toBe(false);
    expect((await getReport(db, report(1).id))!.text).toBe('first');
  });

  test('the list is newest first and each filter narrows it', async () => {
    await insertReport(db, report(1, { kind: 'bug', game_version: '0.6.0' }), {}, 1, at('2026-11-01T00:00:00Z'));
    await insertReport(db, report(2, { kind: 'idea', game_version: '0.6.0' }), {}, 1, at('2026-11-03T00:00:00Z'));
    await insertReport(db, report(3, { kind: 'bug', game_version: '0.7.0' }), {}, 1, at('2026-11-02T00:00:00Z'));
    await setStatus(db, report(3).id, 'done');
    const ids = async (filter: Parameters<typeof listReports>[1]) => (await listReports(db, filter)).map((r) => Number(r.id));
    expect(await ids({})).toEqual([2, 3, 1]);
    expect(await ids({ kind: 'bug' })).toEqual([3, 1]);
    expect(await ids({ status: 'done' })).toEqual([3]);
    expect(await ids({ version: '0.6.0' })).toEqual([2, 1]);
    expect(await ids({ kind: 'idea', version: '0.7.0' })).toEqual([]);
    expect(await versions(db)).toEqual(['0.7.0', '0.6.0']);
  });

  test('text that looks like SQL is stored as it is', async () => {
    const text = "'); DROP TABLE reports; --";
    await insertReport(db, report(1, { text }), {}, 1, new Date());
    expect((await getReport(db, report(1).id))!.text).toBe(text);
  });

  test('delete returns the stored paths and removes the report', async () => {
    await insertReport(db, report(1), { log: 'a', save: 'b' }, 1, new Date());
    expect((await deleteReport(db, report(1).id)).sort()).toEqual(['a', 'b']);
    expect(await getReport(db, report(1).id)).toBeNull();
    expect(await deleteReport(db, report(1).id)).toEqual([]);
  });

  test('usage sums stored bytes and counts uploads of the last 30 days', async () => {
    const now = at('2026-11-26T00:00:00Z');
    expect(await usage(db, now)).toEqual({ bytes: 0, last30Days: 0 });
    await insertReport(db, report(1), {}, 100, now);
    await insertReport(db, report(2), {}, 23, now);
    await db.query('INSERT INTO uploads (token, at) VALUES ($1, $2), ($3, $4)', ['a', '2026-11-20T00:00:00Z', 'b', '2026-10-01T00:00:00Z']);
    expect(await usage(db, now)).toEqual({ bytes: 123, last30Days: 1 });
  });
});

describe('members', () => {
  test('add and remove ignore case and spaces', async () => {
    await addMember(db, '  Tester@Example.COM ');
    await addMember(db, 'tester@example.com');
    expect(await listMembers(db)).toEqual(['tester@example.com']);
    expect(await isMember(db, 'TESTER@example.com')).toBe(true);
    await removeMember(db, 'Tester@example.com');
    expect(await isMember(db, 'tester@example.com')).toBe(false);
  });
});
