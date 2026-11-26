import { z } from 'zod';

/** Largest sizes in bytes. A Vercel Function accepts 4.5 MB, so a whole request stays under that. */
export const SIZE = { screenshot: 2_500_000, log: 300_000, save: 1_000_000, fields: 20_000, body: 4_000_000 } as const;

/** Upload limits. `all30d` keeps the Blob free plan under its 2,000 uploads (three files per report). */
export const RATE = { addressHour: 5, addressDay: 20, allDay: 100, all30d: 500, storedBytes: 800_000_000 } as const;

export const KINDS = ['bug', 'idea'] as const;
export const STATUSES = ['new', 'looking', 'done', 'closed'] as const;
export const FILE_NAMES = ['screenshot', 'log', 'save'] as const;

export type Kind = (typeof KINDS)[number];
export type Status = (typeof STATUSES)[number];
export type FileName = (typeof FILE_NAMES)[number];

export const FILE_TYPES: Record<FileName, { ext: string; type: string }> = {
  screenshot: { ext: 'png', type: 'image/png' },
  log: { ext: 'txt', type: 'text/plain; charset=utf-8' },
  save: { ext: 'alz', type: 'application/octet-stream' },
};

// Postgres refuses NUL in text, and terminal control codes have no place on a page.
const clean = (s: string) => s.replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g, '');
const chars = (s: string) => [...s].length;
const line = (max: number) => z.string().transform(clean).refine((s) => chars(s) <= max);

const schema = z.object({
  id: z.string().regex(/^[0-9a-f]{32}$/),
  kind: z.enum(KINDS),
  text: z.string().transform((s) => clean(s).trim()).refine((s) => s.length > 0 && chars(s) <= 4000),
  name: line(40).optional(),
  game_version: z.string().max(32).regex(/^\d+\.\d+\.\d+([-+][0-9A-Za-z.]+)?$/),
  os: line(64).refine((s) => s.length > 0),
  gpu: line(128).optional(),
  window: line(32).optional(),
  zoom: z.number().min(0.2).max(4).optional(),
  // a u64 seed does not fit a JavaScript number
  seed: z.string().regex(/^\d{1,20}$/).optional(),
  world_size: z.number().int().min(1).max(65536).optional(),
  day: z.number().int().min(0).max(1_000_000_000).optional(),
  mode: z.enum(['solo', 'host', 'guest', 'menu']),
  crash: z.boolean(),
  omitted: z.array(z.enum(FILE_NAMES)).max(FILE_NAMES.length).default([]),
});

export type Fields = z.infer<typeof schema>;

export function parseFields(json: string): Fields | null {
  if (new TextEncoder().encode(json).length > SIZE.fields) return null;
  try {
    const result = schema.safeParse(JSON.parse(json));
    return result.success ? result.data : null;
  } catch {
    return null;
  }
}

const PNG_HEAD = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52];

function isScreenshot(b: Uint8Array): boolean {
  if (b.length < 33 || !PNG_HEAD.every((v, i) => b[i] === v)) return false;
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const width = view.getUint32(16);
  const height = view.getUint32(20);
  return width >= 1 && width <= 1920 && height >= 1 && height <= 4320;
}

function isLog(b: Uint8Array): boolean {
  if (b.includes(0)) return false;
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(b);
    return true;
  } catch {
    return false;
  }
}

const isSave = (b: Uint8Array) => b.length >= 16 && 'ALAZ'.split('').every((c, i) => b[i] === c.charCodeAt(0));

/** Is this a file the site will keep under this name? The sender's file name and type are never trusted. */
export function checkFile(name: FileName, bytes: Uint8Array): boolean {
  if (bytes.length > SIZE[name]) return false;
  return { screenshot: isScreenshot, log: isLog, save: isSave }[name](bytes);
}
