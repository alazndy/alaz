import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { diskStore, settings } from '@/lib/adapters';

const secret = 'k'.repeat(32);
const cloud = { DATABASE_URL: 'postgres://x', IP_HASH_SECRET: secret };
const local = { FEEDBACK_LOCAL_DIR: '/tmp/x', IP_HASH_SECRET: secret };

describe('settings', () => {
  test('cloud needs the database and a long secret', () => {
    expect(settings({ ...cloud, ADMIN_EMAIL: 'o@example.com' })).toEqual({ kind: 'cloud', databaseUrl: 'postgres://x', secret, ownerEmail: 'o@example.com' });
    expect(settings({ IP_HASH_SECRET: secret })).toBeNull();
    expect(settings({ DATABASE_URL: 'postgres://x' })).toBeNull();
    expect(settings({ ...cloud, IP_HASH_SECRET: 'short' })).toBeNull();
    expect(settings({})).toBeNull();
  });
  test('local is for development only', () => {
    expect(settings(local)).toEqual({ kind: 'local', dir: '/tmp/x', secret, ownerEmail: undefined });
    expect(settings({ ...local, NODE_ENV: 'production' })).toBeNull();
    expect(settings({ ...local, VERCEL: '1' })).toBeNull();
    // a stray local setting must not quietly fall back to the real database
    expect(settings({ ...local, ...cloud, VERCEL: '1' })).toBeNull();
    expect(settings({ ...local, IP_HASH_SECRET: 'short' })).toBeNull();
  });
});

describe('disk store', () => {
  let dir: string;
  beforeAll(async () => {
    dir = await mkdtemp(path.join(tmpdir(), 'feedback-store-'));
  });
  afterAll(() => rm(dir, { recursive: true, force: true }));

  test('keeps, returns, and deletes a file', async () => {
    const store = diskStore(dir);
    const stored = await store.put('reports/a/log.txt', new TextEncoder().encode('x'), 'text/plain');
    expect(new TextDecoder().decode((await store.get(stored))!)).toBe('x');
    const second = await store.put('reports/a/log.txt', new TextEncoder().encode('y'), 'text/plain');
    expect(second).not.toBe(stored);
    await store.del([stored, second, 'reports/a/none']);
    expect(await store.get(stored)).toBeNull();
    expect(await readdir(path.join(dir, 'reports', 'a'))).toEqual([]);
  });
  test.each(['../x', 'reports/../../x', '/etc/passwd', ''])('refuses the path %j', async (bad) => {
    const store = diskStore(dir);
    await expect(store.put(bad, new Uint8Array(), 'text/plain')).rejects.toThrow();
    await expect(store.get(bad)).rejects.toThrow();
    await expect(store.del([bad])).rejects.toThrow();
  });
});
