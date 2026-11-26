/** The first bytes of a PNG: signature, then the IHDR chunk with the size. */
export function png(width: number, height: number, length = 33): Uint8Array {
  const out = new Uint8Array(Math.max(length, 33));
  out.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const view = new DataView(out.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  return out;
}

/** A save file header followed by zeros. */
export function save(length = 16): Uint8Array {
  const out = new Uint8Array(length);
  out.set(new TextEncoder().encode('ALAZ'));
  return out;
}

export const fields = {
  id: 'a'.repeat(32),
  kind: 'bug',
  text: 'Ağaç yok oldu',
  game_version: '0.6.0',
  os: 'linux x86_64',
  mode: 'solo',
  crash: false,
} as const;
