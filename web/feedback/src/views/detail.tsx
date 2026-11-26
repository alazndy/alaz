import type { Viewer } from '@/lib/access';
import { FILE_NAMES, STATUSES } from '@/lib/report';
import type { ReportRow } from '@/lib/store';
import { FILE_LABELS, KIND_NAMES, STATUS_NAMES, when } from '@/lib/text';

type Action = (form: FormData) => Promise<void>;
type Props = { row: ReportRow; log: string | null; viewer: Viewer; actions: { status: Action; remove: Action }; confirming: boolean };

const MODES = { solo: 'Tek oyuncu', host: 'Sunucu', guest: 'Misafir', menu: 'Ana menü' } as const;

export function DetailView({ row, log, viewer, actions, confirming }: Props) {
  const file = (name: string) => `/reports/${row.id}/file/${name}`;
  const owner = viewer.role === 'owner';
  const facts: [string, string | number | undefined][] = [
    ['Gönderen', row.name],
    ['Tarih', when(row.created_at)],
    ['Sürüm', row.game_version],
    ['Oyun şekli', MODES[row.mode]],
    ['Dünya tohumu', row.seed],
    ['Dünya boyutu', row.world_size],
    ['Gün', row.day],
    ['İşletim sistemi', row.os],
    ['Ekran kartı', row.gpu],
    ['Pencere', row.window],
    ['Arayüz büyütmesi', row.zoom],
    ['Rapor kimliği', row.id],
  ];
  return (
    <main>
      <a href="/reports">← Raporlar</a>
      <header>
        <h1>
          <span className={`tag ${row.kind}`}>{KIND_NAMES[row.kind]}</span>
          {row.crash && <span className="tag crash">Çökme</span>}
          {STATUS_NAMES[row.status]}
        </h1>
      </header>
      <p className="text">{row.text}</p>
      {owner && (
        <form className="filters" action={actions.status}>
          {STATUSES.map((s) => (
            <button key={s} type="submit" name="status" value={s} aria-pressed={s === row.status}>
              {STATUS_NAMES[s]}
            </button>
          ))}
        </form>
      )}
      {row.files.screenshot ? (
        // a plain img: the file needs the signed-in cookie, which an image optimiser would not send
        <a href={file('screenshot')}>
          <img src={file('screenshot')} alt="Ekran görüntüsü" />
        </a>
      ) : null}
      <dl>
        {facts
          .filter(([, value]) => value !== undefined && value !== '')
          .map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
      </dl>
      <ul>
        {FILE_NAMES.map((name) => (
          <li key={name}>
            {row.files[name] ? (
              <a href={file(name)}>{FILE_LABELS[name]} indir</a>
            ) : row.omitted.includes(name) ? (
              <span className="muted">{FILE_LABELS[name]} sığmadı</span>
            ) : (
              <span className="muted">{FILE_LABELS[name]} yok</span>
            )}
          </li>
        ))}
      </ul>
      {log !== null && <pre>{log}</pre>}
      {owner &&
        (confirming ? (
          <form className="danger" action={actions.remove}>
            <p>Rapor ve dosyaları kalıcı olarak silinecek.</p>
            <button type="submit">Evet, sil</button>
            <a href={`/reports/${row.id}`}>Vazgeç</a>
          </form>
        ) : (
          <p>
            <a className="danger" href={`/reports/${row.id}?confirm=delete`}>
              Raporu sil
            </a>
          </p>
        ))}
    </main>
  );
}
