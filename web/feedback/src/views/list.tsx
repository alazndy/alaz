import type { Viewer } from '@/lib/access';
import { KINDS, RATE, STATUSES } from '@/lib/report';
import { LIST_LIMIT, type Filter, type ReportRow } from '@/lib/store';
import { KIND_NAMES, megabytes, STATUS_NAMES, when } from '@/lib/text';

type Props = { rows: ReportRow[]; versions: string[]; filter: Filter; usage: { bytes: number; last30Days: number }; viewer: Viewer };

function Select({ name, label, value, options }: { name: string; label: string; value?: string; options: [string, string][] }) {
  return (
    <label>
      {label}
      <select name={name} defaultValue={value ?? ''}>
        <option value="">Hepsi</option>
        {options.map(([key, text]) => (
          <option key={key} value={key}>
            {text}
          </option>
        ))}
      </select>
    </label>
  );
}

export function ListView({ rows, versions, filter, usage, viewer }: Props) {
  return (
    <main>
      <header>
        <h1>Raporlar</h1>
        {viewer.role === 'owner' && <a href="/members">Davetliler</a>}
      </header>
      <p className="muted">
        Depolama: {megabytes(usage.bytes)} MB / {megabytes(RATE.storedBytes)} MB · Son 30 gün: {usage.last30Days} / {RATE.all30d}
      </p>
      <form className="filters" method="get" action="/reports">
        <Select name="kind" label="Tür" value={filter.kind} options={KINDS.map((k) => [k, KIND_NAMES[k]])} />
        <Select name="status" label="Durum" value={filter.status} options={STATUSES.map((s) => [s, STATUS_NAMES[s]])} />
        <Select name="version" label="Sürüm" value={filter.version} options={versions.map((v) => [v, v])} />
        <button type="submit">Süz</button>
      </form>
      {rows.length === 0 && <p>Rapor yok.</p>}
      <ul className="reports">
        {rows.map((r) => (
          <li key={r.id}>
            <a href={`/reports/${r.id}`}>
              <span className={`tag ${r.kind}`}>{KIND_NAMES[r.kind]}</span>
              {r.crash && <span className="tag crash">Çökme</span>}
              <span className="summary">{[...r.text].slice(0, 160).join('')}</span>
              <span className="muted">
                {STATUS_NAMES[r.status]} · {when(r.created_at)} · {r.game_version}
                {r.name ? ` · ${r.name}` : ''}
              </span>
            </a>
          </li>
        ))}
      </ul>
      {rows.length === LIST_LIMIT && <p className="muted">En yeni {LIST_LIMIT} rapor gösteriliyor.</p>}
    </main>
  );
}
