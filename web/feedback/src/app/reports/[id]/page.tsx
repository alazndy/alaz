import { notFound } from 'next/navigation';
import { removeReportAction, setStatusAction } from '@/app/actions';
import { gate } from '@/lib/session';
import { getReport } from '@/lib/store';
import { Blocked } from '@/views/blocked';
import { DetailView } from '@/views/detail';

type Props = { params: Promise<{ id: string }>; searchParams: Promise<{ confirm?: string }> };

export default async function Report({ params, searchParams }: Props) {
  const { d, viewer, reader } = await gate();
  if (!reader) return <Blocked email={viewer.email} />;
  const row = await getReport(d.db, (await params).id);
  if (!row) notFound();
  // a store that is briefly away must not hide the report itself
  const bytes = row.files.log ? await d.files.get(row.files.log).catch(() => null) : null;
  return (
    <DetailView
      row={row}
      log={bytes ? new TextDecoder().decode(bytes) : null}
      viewer={viewer}
      actions={{ status: setStatusAction.bind(null, row.id), remove: removeReportAction.bind(null, row.id) }}
      confirming={(await searchParams).confirm === 'delete'}
    />
  );
}
