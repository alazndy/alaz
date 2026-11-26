import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, test } from 'vitest';
import type { Viewer } from '@/lib/access';
import { parseFields } from '@/lib/report';
import type { ReportRow } from '@/lib/store';
import { Blocked } from '@/views/blocked';
import { DetailView } from '@/views/detail';
import { ListView } from '@/views/list';
import { MembersView } from '@/views/members';
import { fields } from './helpers';

const owner: Viewer = { email: 'owner@example.com', role: 'owner' };
const tester: Viewer = { email: 'tester@example.com', role: 'tester' };
const row = (over: Partial<ReportRow> = {}): ReportRow => ({
  ...parseFields(JSON.stringify(fields))!,
  status: 'new',
  created_at: new Date('2026-11-26T09:30:00Z'),
  files: { log: 'a', screenshot: 'b', save: 'c' },
  bytes: 10,
  ...over,
});
const noop = async () => {};
const actions = { status: noop, remove: noop };
const detail = (viewer: Viewer, over: Partial<ReportRow> = {}, extra: { log?: string | null; confirming?: boolean } = {}) =>
  renderToStaticMarkup(<DetailView row={row(over)} log={extra.log ?? null} viewer={viewer} actions={actions} confirming={extra.confirming ?? false} />);

describe('detail', () => {
  test('a stranger cannot put markup on the page', () => {
    const html = detail(owner, { text: '<script>alert(1)</script>', name: '<b>x</b>' }, { log: '<img src=x onerror=alert(1)>' });
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
    // React adds a script of its own for forms, so look for the stranger's
    expect(html).not.toContain('<script>alert');
    expect(html).not.toContain('<img src=x');
    expect(html).not.toContain('<b>x</b>');
  });
  test('shows the picture, the fields and the downloads', () => {
    const html = detail(tester, { seed: '42', gpu: 'Test GPU', crash: true });
    expect(html).toContain(`src="/reports/${fields.id}/file/screenshot"`);
    expect(html).toContain(`href="/reports/${fields.id}/file/save"`);
    expect(html).toContain(`href="/reports/${fields.id}/file/log"`);
    expect(html).toContain('Test GPU');
    expect(html).toContain('42');
    expect(html).toContain('Çökme');
    expect(html).toContain('26.11.2026 12:30');
  });
  test('names what was not sent', () => {
    const html = detail(tester, { files: {}, omitted: ['save'] });
    expect(html).toContain('Ekran görüntüsü yok');
    expect(html).toContain('Log yok');
    expect(html).toContain('Kayıt dosyası sığmadı');
    expect(html).not.toContain('/file/');
  });
  test('only the owner gets status and delete buttons', () => {
    expect(detail(tester)).not.toContain('<form');
    const html = detail(owner);
    for (const name of ['Yeni', 'Bakılıyor', 'Çözüldü', 'Kapatıldı']) expect(html).toMatch(new RegExp(`<button[^>]*>${name}</button>`));
    expect(html).toContain(`href="/reports/${fields.id}?confirm=delete"`);
    expect(html).not.toContain('Evet, sil');
    expect(detail(owner, {}, { confirming: true })).toContain('Evet, sil');
    expect(detail(tester, {}, { confirming: true })).not.toContain('Evet, sil');
  });
});

describe('list', () => {
  const list = (viewer: Viewer, rows: ReportRow[] = [row()]) =>
    renderToStaticMarkup(<ListView rows={rows} versions={['0.6.0']} filter={{ kind: 'bug' }} usage={{ bytes: 12_340_000, last30Days: 7 }} viewer={viewer} />);

  test('shows filters, usage and rows', () => {
    const html = list(tester, [row({ text: '<i>x</i>' })]);
    for (const name of ['Yeni', 'Bakılıyor', 'Çözüldü', 'Kapatıldı']) expect(html).toContain(`>${name}</option>`);
    expect(html).toContain('12,3 MB / 800 MB');
    expect(html).toContain('7 / 500');
    expect(html).toContain(`href="/reports/${fields.id}"`);
    expect(html).toContain('&lt;i&gt;x&lt;/i&gt;');
    expect(html).toMatch(/<option value="bug" selected="">Hata<\/option>/);
  });
  test('says when there is nothing', () => {
    expect(list(tester, [])).toContain('Rapor yok');
  });
  test('only the owner sees the invitations link', () => {
    expect(list(tester)).not.toContain('href="/members"');
    expect(list(owner)).toContain('href="/members"');
  });
});

describe('other pages', () => {
  test('a person without an invitation is told so', () => {
    const html = renderToStaticMarkup(<Blocked email="x@example.com" />);
    expect(html).toContain('Davetli değilsin');
    expect(html).toContain('x@example.com');
  });
  test('invitations can be added and removed', () => {
    const html = renderToStaticMarkup(<MembersView emails={['a@example.com']} error={false} actions={{ add: noop, remove: noop }} />);
    expect(html).toContain('a@example.com');
    expect(html).toContain('type="email"');
    expect(html).toContain('Kaldır');
    expect(html).not.toContain('geçerli bir e-posta');
    expect(renderToStaticMarkup(<MembersView emails={[]} error actions={{ add: noop, remove: noop }} />)).toContain('geçerli bir e-posta');
  });
});
