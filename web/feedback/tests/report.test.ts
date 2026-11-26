import { describe, expect, test } from 'vitest';
import { checkFile, parseFields } from '@/lib/report';
import { fields, png, save } from './helpers';

const parse = (over: object) => parseFields(JSON.stringify({ ...fields, ...over }));
const bytes = (text: string) => new TextEncoder().encode(text);

describe('fields', () => {
  test('accepts a minimal report and fills omitted', () => {
    expect(parse({})).toMatchObject({ ...fields, omitted: [] });
  });
  test('accepts every optional field', () => {
    const full = { name: 'Ece', gpu: 'RTX', window: '1280x720', zoom: 1.5, seed: '18446744073709551615', world_size: 256, day: 3, omitted: ['save'] };
    expect(parse(full)).toMatchObject(full);
  });
  test('drops unknown keys', () => {
    expect(parse({ admin: true })).not.toHaveProperty('admin');
  });
  test.each([
    [{ id: 'A'.repeat(32) }], [{ id: '../x' }], [{ kind: 'other' }], [{ text: '   ' }], [{ text: 'x'.repeat(4001) }],
    [{ name: 'x'.repeat(41) }], [{ game_version: 'latest' }], [{ seed: '12a' }], [{ mode: 'x' }], [{ crash: 'no' }],
    [{ zoom: 9 }], [{ day: -1 }], [{ omitted: ['passwords'] }], [{ os: '' }],
  ])('rejects %j', (over) => {
    expect(parse(over)).toBeNull();
  });
  test('counts characters, not UTF-16 units', () => {
    expect(parse({ text: '🪨'.repeat(4000) })).not.toBeNull();
  });
  test('removes control characters but keeps lines', () => {
    expect(parse({ text: 'a\u0000b\u001b\nc\td' })!.text).toBe('ab\nc\td');
  });
  test('rejects text that is not JSON, not an object, or too long', () => {
    expect(parseFields('{')).toBeNull();
    expect(parseFields('[]')).toBeNull();
    expect(parseFields(JSON.stringify(fields) + ' '.repeat(20_000))).toBeNull();
  });
});

describe('files', () => {
  test('screenshot needs a PNG header and a width up to 1920', () => {
    expect(checkFile('screenshot', png(1920, 1080))).toBe(true);
    expect(checkFile('screenshot', png(1921, 1080))).toBe(false);
    expect(checkFile('screenshot', png(0, 1080))).toBe(false);
    expect(checkFile('screenshot', png(100, 4321))).toBe(false);
    expect(checkFile('screenshot', bytes('<svg xmlns="http://www.w3.org/2000/svg"/>'))).toBe(false);
    expect(checkFile('screenshot', png(10, 10, 2_500_001))).toBe(false);
  });
  test('log must be text and small', () => {
    expect(checkFile('log', bytes('merhaba ğ'))).toBe(true);
    expect(checkFile('log', new Uint8Array([0xff, 0xfe]))).toBe(false);
    expect(checkFile('log', new Uint8Array([0x61, 0]))).toBe(false);
    expect(checkFile('log', bytes('x'.repeat(300_001)))).toBe(false);
  });
  test('save needs the magic', () => {
    expect(checkFile('save', save())).toBe(true);
    expect(checkFile('save', save(15))).toBe(false);
    expect(checkFile('save', new Uint8Array(16))).toBe(false);
    expect(checkFile('save', save(1_000_001))).toBe(false);
  });
});
