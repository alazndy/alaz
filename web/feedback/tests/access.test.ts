import { beforeEach, describe, expect, test } from 'vitest';
import { canManage, canRead, viewerFor, type Viewer } from '@/lib/access';
import { changeStatus, Forbidden, Invalid, invite, removeReport, uninvite } from '@/lib/admin';
import type { Db } from '@/lib/db';
import type { FileStore } from '@/lib/files';
import { parseFields } from '@/lib/report';
import { serveFile } from '@/lib/serve';
import { addMember, getReport, insertReport, listMembers, removeMember } from '@/lib/store';
import { fields, memoryStore, testDb } from './helpers';

const OWNER = 'owner@example.com';
const owner: Viewer = { email: OWNER, role: 'owner' };
const tester: Viewer = { email: 'tester@example.com', role: 'tester' };
const stranger: Viewer = { email: 'x@example.com', role: 'none' };
const ID = fields.id;

let db: Db;
let files: FileStore;
let held: Map<string, Uint8Array>;
beforeEach(async () => {
  db = await testDb();
  ({ store: files, held } = memoryStore());
});

async function stored() {
  const log = await files.put('reports/a/log.txt', new TextEncoder().encode('<b>log</b>'), 'text/plain');
  const screenshot = await files.put('reports/a/screenshot.png', new Uint8Array([1, 2]), 'image/png');
  const save = await files.put('reports/a/save.alz', new Uint8Array([3]), 'application/octet-stream');
  await insertReport(db, parseFields(JSON.stringify(fields))!, { log, screenshot, save }, 13, new Date());
}

describe('who is looking', () => {
  test('owner, tester, stranger, nobody', async () => {
    await addMember(db, tester.email);
    expect(await viewerFor(db, 'Owner@Example.com ', OWNER)).toEqual(owner);
    expect(await viewerFor(db, 'TESTER@example.com', OWNER)).toEqual(tester);
    expect(await viewerFor(db, stranger.email, OWNER)).toEqual(stranger);
    expect(await viewerFor(db, null, OWNER)).toBeNull();
  });
  test('a removed member loses access on the next look', async () => {
    await addMember(db, tester.email);
    expect((await viewerFor(db, tester.email, OWNER))!.role).toBe('tester');
    await removeMember(db, 'Tester@Example.com');
    expect((await viewerFor(db, tester.email, OWNER))!.role).toBe('none');
  });
  test.each([undefined, '', '   '])('an owner setting of %j matches nobody', async (setting) => {
    expect((await viewerFor(db, 'a@example.com', setting))!.role).toBe('none');
    expect(await viewerFor(db, '', setting)).toBeNull();
  });
  test('rights', () => {
    expect([owner, tester, stranger, null].map(canRead)).toEqual([true, true, false, false]);
    expect([owner, tester, stranger, null].map(canManage)).toEqual([true, false, false, false]);
  });
});

describe('files', () => {
  const get = (viewer: Viewer | null, name: string, id: string = ID) => serveFile({ db, files }, viewer, id, name);

  test('refuses before it looks anything up', async () => {
    await stored();
    expect((await get(null, 'log')).status).toBe(401);
    expect((await get(stranger, 'log')).status).toBe(403);
    expect((await get(stranger, 'nothing', 'zz')).status).toBe(403);
  });
  test.each([
    ['screenshot', 'image/png', 'inline'],
    ['log', 'text/plain; charset=utf-8', `attachment; filename="${ID}.txt"`],
    ['save', 'application/octet-stream', `attachment; filename="${ID}.alz"`],
  ])('serves %s with safe headers', async (name, type, disposition) => {
    await stored();
    const res = await get(tester, name);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toBe(type);
    expect(res.headers.get('content-disposition')).toBe(disposition);
    expect(res.headers.get('x-content-type-options')).toBe('nosniff');
    expect(res.headers.get('cache-control')).toBe('private, no-store');
    expect(res.headers.get('content-security-policy')).toBe("default-src 'none'; sandbox");
    expect((await res.arrayBuffer()).byteLength).toBeGreaterThan(0);
  });
  test('answers 404 for an unknown report, name, or lost file', async () => {
    await stored();
    expect((await get(owner, 'log', 'b'.repeat(32))).status).toBe(404);
    expect((await get(owner, 'passwd')).status).toBe(404);
    expect((await get(owner, '__proto__')).status).toBe(404);
    held.clear();
    expect((await get(owner, 'log')).status).toBe(404);
  });
});

describe('owner actions', () => {
  const deps = () => ({ db, files });

  test('a tester and a stranger can change nothing', async () => {
    await stored();
    await addMember(db, tester.email);
    for (const viewer of [tester, stranger, null]) {
      await expect(changeStatus(deps(), viewer, ID, 'done')).rejects.toThrow(Forbidden);
      await expect(removeReport(deps(), viewer, ID)).rejects.toThrow(Forbidden);
      await expect(invite(deps(), viewer, 'new@example.com')).rejects.toThrow(Forbidden);
      await expect(uninvite(deps(), viewer, tester.email)).rejects.toThrow(Forbidden);
    }
    expect((await getReport(db, ID))!.status).toBe('new');
    expect(await listMembers(db)).toEqual([tester.email]);
    expect(held.size).toBe(3);
  });
  test('the owner changes status, invites, removes', async () => {
    await stored();
    await changeStatus(deps(), owner, ID, 'looking');
    expect((await getReport(db, ID))!.status).toBe('looking');
    await invite(deps(), owner, ' New@Example.com ');
    expect(await listMembers(db)).toEqual(['new@example.com']);
    await uninvite(deps(), owner, 'new@example.com');
    expect(await listMembers(db)).toEqual([]);
    await removeReport(deps(), owner, ID);
    expect(await getReport(db, ID)).toBeNull();
    expect(held.size).toBe(0);
  });
  test('bad values are refused', async () => {
    await stored();
    await expect(changeStatus(deps(), owner, ID, 'deleted')).rejects.toThrow(Invalid);
    await expect(invite(deps(), owner, 'not an email')).rejects.toThrow(Invalid);
    await expect(invite(deps(), owner, `${'a'.repeat(250)}@example.com`)).rejects.toThrow(Invalid);
  });
  test('a report stays while its files cannot be deleted', async () => {
    await stored();
    const stuck: FileStore = { ...files, del: async () => Promise.reject(new Error('down')) };
    await expect(removeReport({ db, files: stuck }, owner, ID)).rejects.toThrow('down');
    expect(await getReport(db, ID)).not.toBeNull();
  });
  test('removing a report that is gone is quiet', async () => {
    await expect(removeReport(deps(), owner, ID)).resolves.toBeUndefined();
  });
});
