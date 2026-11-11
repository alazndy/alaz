#!/usr/bin/env python3
"""Builds A.L.A.Z's pixel font (crates/game/assets/alaz-pixel.ttf) from the glyph drawings below.

No dependencies: the TrueType tables are written by hand. Every lit pixel run of a row becomes one square-cornered
rectangle contour, so the font draws as hard blocks at whole multiples of its pixel size.

Grid: a capital is 7 pixels tall, lowercase x-height 5; two rows above the capitals hold accents, two below the
baseline hold descenders. In the drawings `#` is lit, anything else is off; rows are listed top to bottom.
"""
import struct
import sys

PX = 128  # font units per pixel
ASC = 9   # pixels above the baseline (7 cap rows + 2 accent rows)
DESC = 2  # pixels below the baseline
UPM = (ASC + DESC) * PX

# the cap-area drawings: 7 rows; `_` lines below the 7 are descender rows
G = {}


def glyph(ch, *rows):
    G[ch] = list(rows)


# --- capitals
glyph('A', '.###.', '#...#', '#...#', '#####', '#...#', '#...#', '#...#')
glyph('B', '####.', '#...#', '#...#', '####.', '#...#', '#...#', '####.')
glyph('C', '.###.', '#...#', '#....', '#....', '#....', '#...#', '.###.')
glyph('D', '####.', '#...#', '#...#', '#...#', '#...#', '#...#', '####.')
glyph('E', '#####', '#....', '#....', '####.', '#....', '#....', '#####')
glyph('F', '#####', '#....', '#....', '####.', '#....', '#....', '#....')
glyph('G', '.###.', '#...#', '#....', '#.###', '#...#', '#...#', '.####')
glyph('H', '#...#', '#...#', '#...#', '#####', '#...#', '#...#', '#...#')
glyph('I', '###', '.#.', '.#.', '.#.', '.#.', '.#.', '###')
glyph('J', '..###', '...#.', '...#.', '...#.', '#..#.', '#..#.', '.##..')
glyph('K', '#...#', '#..#.', '#.#..', '##...', '#.#..', '#..#.', '#...#')
glyph('L', '#....', '#....', '#....', '#....', '#....', '#....', '#####')
glyph('M', '#...#', '##.##', '#.#.#', '#.#.#', '#...#', '#...#', '#...#')
glyph('N', '#...#', '##..#', '#.#.#', '#..##', '#...#', '#...#', '#...#')
glyph('O', '.###.', '#...#', '#...#', '#...#', '#...#', '#...#', '.###.')
glyph('P', '####.', '#...#', '#...#', '####.', '#....', '#....', '#....')
glyph('Q', '.###.', '#...#', '#...#', '#...#', '#.#.#', '#..#.', '.##.#')
glyph('R', '####.', '#...#', '#...#', '####.', '#.#..', '#..#.', '#...#')
glyph('S', '.####', '#....', '#....', '.###.', '....#', '....#', '####.')
glyph('T', '#####', '..#..', '..#..', '..#..', '..#..', '..#..', '..#..')
glyph('U', '#...#', '#...#', '#...#', '#...#', '#...#', '#...#', '.###.')
glyph('V', '#...#', '#...#', '#...#', '#...#', '#...#', '.#.#.', '..#..')
glyph('W', '#...#', '#...#', '#...#', '#.#.#', '#.#.#', '#.#.#', '.#.#.')
glyph('X', '#...#', '#...#', '.#.#.', '..#..', '.#.#.', '#...#', '#...#')
glyph('Y', '#...#', '#...#', '.#.#.', '..#..', '..#..', '..#..', '..#..')
glyph('Z', '#####', '....#', '...#.', '..#..', '.#...', '#....', '#####')
# --- lowercase (x-height 5: the top two rows of the cap area are for ascenders and accents)
glyph('a', '.....', '.....', '.###.', '....#', '.####', '#...#', '.####')
glyph('b', '#....', '#....', '####.', '#...#', '#...#', '#...#', '####.')
glyph('c', '.....', '.....', '.###.', '#....', '#....', '#....', '.###.')
glyph('d', '....#', '....#', '.####', '#...#', '#...#', '#...#', '.####')
glyph('e', '.....', '.....', '.###.', '#...#', '#####', '#....', '.###.')
glyph('f', '..##', '.#..', '###.', '.#..', '.#..', '.#..', '.#..')
glyph('g', '.....', '.....', '.####', '#...#', '#...#', '.####', '....#', '_', '.###.')
glyph('h', '#....', '#....', '####.', '#...#', '#...#', '#...#', '#...#')
glyph('i', '#', '.', '#', '#', '#', '#', '#')
glyph('j', '..#', '...', '..#', '..#', '..#', '..#', '..#', '_', '##.')
glyph('k', '#...', '#...', '#..#', '#.#.', '##..', '#.#.', '#..#')
glyph('l', '##', '.#', '.#', '.#', '.#', '.#', '.#')
glyph('m', '.....', '.....', '##.#.', '#.#.#', '#.#.#', '#.#.#', '#.#.#')
glyph('n', '.....', '.....', '####.', '#...#', '#...#', '#...#', '#...#')
glyph('o', '.....', '.....', '.###.', '#...#', '#...#', '#...#', '.###.')
glyph('p', '.....', '.....', '####.', '#...#', '#...#', '####.', '#....', '_', '#....')
glyph('q', '.....', '.....', '.####', '#...#', '#...#', '.####', '....#', '_', '....#')
glyph('r', '....', '....', '#.##', '##..', '#...', '#...', '#...')
glyph('s', '.....', '.....', '.####', '#....', '.###.', '....#', '####.')
glyph('t', '.#..', '.#..', '###.', '.#..', '.#..', '.#..', '..##')
glyph('u', '.....', '.....', '#...#', '#...#', '#...#', '#...#', '.####')
glyph('v', '.....', '.....', '#...#', '#...#', '#...#', '.#.#.', '..#..')
glyph('w', '.....', '.....', '#...#', '#...#', '#.#.#', '#.#.#', '.#.#.')
glyph('x', '.....', '.....', '#...#', '.#.#.', '..#..', '.#.#.', '#...#')
glyph('y', '.....', '.....', '#...#', '#...#', '#...#', '.####', '....#', '_', '.###.')
glyph('z', '.....', '.....', '#####', '...#.', '..#..', '.#...', '#####')
# --- digits
glyph('0', '.###.', '#...#', '#..##', '#.#.#', '##..#', '#...#', '.###.')
glyph('1', '.#.', '##.', '.#.', '.#.', '.#.', '.#.', '###')
glyph('2', '.###.', '#...#', '....#', '...#.', '..#..', '.#...', '#####')
glyph('3', '####.', '....#', '....#', '.###.', '....#', '....#', '####.')
glyph('4', '...#.', '..##.', '.#.#.', '#..#.', '#####', '...#.', '...#.')
glyph('5', '#####', '#....', '####.', '....#', '....#', '#...#', '.###.')
glyph('6', '.###.', '#....', '#....', '####.', '#...#', '#...#', '.###.')
glyph('7', '#####', '....#', '...#.', '..#..', '.#...', '.#...', '.#...')
glyph('8', '.###.', '#...#', '#...#', '.###.', '#...#', '#...#', '.###.')
glyph('9', '.###.', '#...#', '#...#', '.####', '....#', '....#', '.###.')
# --- punctuation and signs
glyph(' ', '...', '...', '...', '...', '...', '...', '...')
glyph('.', '.', '.', '.', '.', '.', '.', '#')
glyph(',', '..', '..', '..', '..', '..', '.#', '.#', '_', '#.')
glyph(':', '.', '.', '#', '.', '.', '.', '#')
glyph(';', '..', '..', '.#', '..', '..', '.#', '.#', '_', '#.')
glyph('!', '#', '#', '#', '#', '#', '.', '#')
glyph('?', '.###.', '#...#', '....#', '...#.', '..#..', '.....', '..#..')
glyph("'", '#', '#', '.', '.', '.', '.', '.')
glyph('"', '#.#', '#.#', '...', '...', '...', '...', '...')
glyph('-', '....', '....', '....', '####', '....', '....', '....')
glyph('_', '.....', '.....', '.....', '.....', '.....', '.....', '.....', '_', '#####')
glyph('+', '.....', '..#..', '..#..', '#####', '..#..', '..#..', '.....')
glyph('=', '....', '....', '####', '....', '####', '....', '....')
glyph('/', '....#', '....#', '...#.', '..#..', '.#...', '#....', '#....')
glyph('\\', '#....', '#....', '.#...', '..#..', '...#.', '....#', '....#')
glyph('(', '.#', '#.', '#.', '#.', '#.', '#.', '.#')
glyph(')', '#.', '.#', '.#', '.#', '.#', '.#', '#.')
glyph('[', '##', '#.', '#.', '#.', '#.', '#.', '##')
glyph(']', '##', '.#', '.#', '.#', '.#', '.#', '##')
glyph('<', '...#', '..#.', '.#..', '#...', '.#..', '..#.', '...#')
glyph('>', '#...', '.#..', '..#.', '...#', '..#.', '.#..', '#...')
glyph('%', '##..#', '##..#', '...#.', '..#..', '.#...', '#..##', '#..##')
glyph('#', '.#.#.', '.#.#.', '#####', '.#.#.', '#####', '.#.#.', '.#.#.')
glyph('*', '.....', '#.#.#', '.###.', '#####', '.###.', '#.#.#', '.....')
glyph('&', '.##..', '#..#.', '#.#..', '.#...', '#.#.#', '#..#.', '.##.#')
glyph('@', '.###.', '#...#', '#.###', '#.#.#', '#.###', '#....', '.###.')
glyph('$', '..#..', '.####', '#.#..', '.###.', '..#.#', '####.', '..#..')
glyph('^', '..#..', '.#.#.', '#...#', '.....', '.....', '.....', '.....')
glyph('~', '.....', '.....', '.#..#', '#.##.', '.....', '.....', '.....')
glyph('|', '#', '#', '#', '#', '#', '#', '#')
glyph('{', '..#', '.#.', '.#.', '#..', '.#.', '.#.', '..#')
glyph('}', '#..', '.#.', '.#.', '..#', '.#.', '.#.', '#..')
glyph('`', '#.', '.#', '..', '..', '..', '..', '..')
glyph('°', '.#.', '#.#', '.#.', '...', '...', '...', '...')
glyph('·', '.', '.', '.', '#', '.', '.', '.')
glyph('–', '.....', '.....', '.....', '#####', '.....', '.....', '.....')
glyph('—', '.......', '.......', '.......', '#######', '.......', '.......', '.......')
glyph('…', '.....', '.....', '.....', '.....', '.....', '.....', '#.#.#')
glyph('‰', '##..#....', '##..#....', '...#.....', '..#......', '.#.......', '#..##.##.', '#..##.##.')
glyph('×', '.....', '.....', '#...#', '.#.#.', '..#..', '.#.#.', '#...#')
glyph('→', '.....', '...#.', '....#', '#####', '....#', '...#.', '.....')
glyph('←', '.....', '.#...', '#....', '#####', '#....', '.#...', '.....')
glyph('▶', '#...', '##..', '###.', '####', '###.', '##..', '#...')
glyph('•', '...', '...', '.#.', '###', '.#.', '...', '...')
glyph('✓', '.....', '....#', '...#.', '#.#..', '.#...', '.....', '.....')
# dotless i
glyph('ı', '.', '.', '#', '#', '#', '#', '#')

# --- accented letters, built from a base and a mark. Marks over capitals sit in the two accent rows; over
# lowercase in the free rows above the x-height; cedillas hang in the descender rows.
MARKS = {'dot': ['..#..', '.....'], 'diaeresis': ['.#.#.', '.....'], 'breve': ['#...#', '.###.'],
         'circ': ['..#..', '.#.#.']}


def fit(row, width):
    """a five-wide mark row centred over a glyph `width` wide"""
    if width >= 5:
        return row.center(width, '.')
    cut = (5 - width) // 2
    return row[cut:cut + width]


def with_mark(base, mark, lower):
    rows = list(G[base])
    tail = []
    if '_' in rows:
        k = rows.index('_')
        rows, tail = rows[:k], rows[k:]
    width = max(len(r) for r in rows)
    if mark == 'cedilla':
        mid = width // 2
        hang = ['.' * mid + '#' + '.' * (width - mid - 1), '.' * (mid - 1) + '##' + '.' * (width - mid - 1)]
        return rows + ['_'] + [h[:width] for h in hang]
    m = [fit(r, width) for r in MARKS[mark]]
    if lower:
        # lowercase: the mark takes the two empty rows above the x-height
        return m + rows[2:] + tail
    return ['_above'] + m + rows + tail


for ch, base, mark, lower in [
    ('Ç', 'C', 'cedilla', False), ('ç', 'c', 'cedilla', True),
    ('Ş', 'S', 'cedilla', False), ('ş', 's', 'cedilla', True),
    ('Ğ', 'G', 'breve', False), ('ğ', 'g', 'breve', True),
    ('Ö', 'O', 'diaeresis', False), ('ö', 'o', 'diaeresis', True),
    ('Ü', 'U', 'diaeresis', False), ('ü', 'u', 'diaeresis', True),
    ('İ', 'I', 'dot', False),
    ('Â', 'A', 'circ', False), ('â', 'a', 'circ', True),
    ('Î', 'I', 'circ', False), ('î', 'ı', 'dot', True),
    ('Û', 'U', 'circ', False), ('û', 'u', 'circ', True),
]:
    G[ch] = with_mark(base, mark, lower)


def layout(rows):
    """(pixels as (x, y) with y counted up from the baseline, width)"""
    above = False
    if rows and rows[0] == '_above':
        above = True
        rows = rows[1:]
    if '_' in rows:
        k = rows.index('_')
        body, desc = rows[:k], rows[k + 1:]
    else:
        body, desc = rows, []
    if above:
        marks, body = body[:2], body[2:]
    else:
        marks = []
    width = max([len(r) for r in body + desc + marks] + [1])
    pix = []
    # body: 7 rows, top row at y = 6 above the baseline
    for i, r in enumerate(body):
        y = 6 - i
        for x, c in enumerate(r):
            if c == '#':
                pix.append((x, y))
    for i, r in enumerate(marks):
        y = 8 - i
        for x, c in enumerate(r):
            if c == '#':
                pix.append((x, y))
    for i, r in enumerate(desc):
        y = -1 - i
        for x, c in enumerate(r):
            if c == '#':
                pix.append((x, y))
    return pix, width


def contours(pix):
    """one rectangle per horizontal run of lit pixels, clockwise (TrueType outer contours)"""
    rows = {}
    for x, y in pix:
        rows.setdefault(y, []).append(x)
    out = []
    for y, xs in sorted(rows.items()):
        xs.sort()
        start = prev = xs[0]
        for x in xs[1:] + [None]:
            if x is not None and x == prev + 1:
                prev = x
                continue
            x0, x1, y0, y1 = start * PX, (prev + 1) * PX, y * PX, (y + 1) * PX
            out.append([(x0, y0), (x0, y1), (x1, y1), (x1, y0)])
            if x is not None:
                start = prev = x
    return out


def glyf_data(cs):
    if not cs:
        return b''
    pts = [p for c in cs for p in c]
    xmin, ymin = min(p[0] for p in pts), min(p[1] for p in pts)
    xmax, ymax = max(p[0] for p in pts), max(p[1] for p in pts)
    d = struct.pack('>hhhhh', len(cs), xmin, ymin, xmax, ymax)
    ends, n = [], 0
    for c in cs:
        n += len(c)
        ends.append(n - 1)
    d += b''.join(struct.pack('>H', e) for e in ends)
    d += struct.pack('>H', 0)  # no instructions
    d += bytes([0x01] * len(pts))  # all on-curve, coordinates as int16 deltas
    px = py = 0
    xs, ys = b'', b''
    for x, y in pts:
        xs += struct.pack('>h', x - px)
        ys += struct.pack('>h', y - py)
        px, py = x, y
    d += xs + ys
    if len(d) % 2:
        d += b'\0'
    return d


def build():
    chars = sorted(G.keys(), key=ord)
    glyphs = [('.notdef', [], 4)]
    for ch in chars:
        pix, w = layout(G[ch])
        adv = w + 1 if ch != ' ' else 4
        glyphs.append((ch, contours(pix), adv))
    glyf, loca = b'', [0]
    for _, cs, _ in glyphs:
        glyf += glyf_data(cs)
        loca.append(len(glyf))
    hmtx = b''.join(struct.pack('>Hh', adv * PX, 0) for _, _, adv in glyphs)
    n = len(glyphs)
    maxpts = max([sum(len(c) for c in cs) for _, cs, _ in glyphs] + [1])
    maxcont = max([len(cs) for _, cs, _ in glyphs] + [1])
    tables = {}
    tables['head'] = struct.pack('>IIIIHHqqhhhhHHhhh', 0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0b1011, UPM, 0, 0,
                                 0, -DESC * PX, 10 * PX, ASC * PX, 0, 8, 2, 1, 0)
    tables['hhea'] = struct.pack('>IhhhHhhhhhhhhhhhH', 0x00010000, ASC * PX, -DESC * PX, PX, 10 * PX, 0, 0, 10 * PX,
                                 1, 0, 0, 0, 0, 0, 0, 0, n)
    tables['maxp'] = struct.pack('>IHHHHHHHHHHHHHH', 0x00010000, n, maxpts, maxcont, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
    tables['hmtx'] = hmtx
    tables['loca'] = b''.join(struct.pack('>I', o) for o in loca)
    tables['glyf'] = glyf
    # cmap: format 4 segments, one per character (simple and plenty for ~130 glyphs)
    codes = [(ord(ch), i + 1) for i, ch in enumerate(chars) if ord(ch) <= 0xFFFF]
    segs = [(c, c, gi) for c, gi in codes] + [(0xFFFF, 0xFFFF, 0)]
    segx2 = len(segs) * 2
    search = 2
    while search * 2 <= len(segs):
        search *= 2
    search *= 2
    import math
    sub = struct.pack('>HHHHHHH', 4, 0, 0, segx2, search, int(math.log2(search // 2)), segx2 - search)
    sub += b''.join(struct.pack('>H', e) for _, e, _ in segs) + b'\0\0'
    sub += b''.join(struct.pack('>H', s) for s, _, _ in segs)
    sub += b''.join(struct.pack('>h', (gi - s) % 65536 - (65536 if (gi - s) % 65536 > 32767 else 0)) for s, _, gi in segs)
    sub += b''.join(struct.pack('>H', 0) for _ in segs)
    sub = sub[:2] + struct.pack('>H', len(sub)) + sub[4:]
    tables['cmap'] = struct.pack('>HHHHI', 0, 1, 3, 1, 12) + sub
    names = {1: 'ALAZ Pixel', 2: 'Regular', 3: 'ALAZ Pixel', 4: 'ALAZ Pixel', 5: 'Version 1.0', 6: 'ALAZPixel'}
    recs, strs = b'', b''
    for nid, s in names.items():
        enc = s.encode('utf-16-be')
        recs += struct.pack('>HHHHHH', 3, 1, 0x409, nid, len(enc), len(strs))
        strs += enc
    tables['name'] = struct.pack('>HHH', 0, len(names), 6 + 12 * len(names)) + recs + strs
    tables['post'] = struct.pack('>IIhhIIIII', 0x00030000, 0, -PX, PX, 0, 0, 0, 0, 0)
    os2 = struct.pack('>HhHHHhhhhhhhhhhh', 4, 5 * PX, 400, 5, 0, 5 * PX, 7 * PX, 0, 0, 5 * PX, 7 * PX, 0, 3 * PX,
                      PX, 3 * PX, 0)
    os2 += bytes(10) + struct.pack('>IIII', 1, 0, 0, 0) + b'ALAZ' + struct.pack('>HHH', 0x40, 0x20, 0xFFFF)
    os2 += struct.pack('>hhhHH', ASC * PX, -DESC * PX, PX, ASC * PX, DESC * PX)
    os2 += struct.pack('>II', 1, 0) + struct.pack('>hhHHH', 5 * PX, 7 * PX, 0, 0x20, 0)
    tables['OS/2'] = os2
    # the file: offset table, directory, tables (4-byte aligned)
    tags = sorted(tables.keys())
    nt = len(tags)
    es = 1
    while es * 2 <= nt:
        es *= 2
    head = struct.pack('>IHHHH', 0x00010000, nt, es * 16, int(math.log2(es)), nt * 16 - es * 16)
    off = 12 + 16 * nt
    dir_, body = b'', b''
    for t in tags:
        data = tables[t]
        pad = data + b'\0' * ((4 - len(data) % 4) % 4)
        cs = sum(struct.unpack('>%dI' % (len(pad) // 4), pad)) & 0xFFFFFFFF
        dir_ += struct.pack('>4sIII', t.encode(), cs, off + len(body), len(data))
        body += pad
    return head + dir_ + body


if __name__ == '__main__':
    out = sys.argv[1] if len(sys.argv) > 1 else 'crates/game/assets/alaz-pixel.ttf'
    data = build()
    with open(out, 'wb') as f:
        f.write(data)
    print(f'{out}: {len(G)} glyphs, {len(data)} bytes')
