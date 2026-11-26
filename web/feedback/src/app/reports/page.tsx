import { sweep } from '@/lib/limits';
import { KINDS, STATUSES, type Kind, type Status } from '@/lib/report';
import { gate } from '@/lib/session';
import { listReports, usage, versions, type Filter } from '@/lib/store';
import { Blocked } from '@/views/blocked';
import { ListView } from '@/views/list';

type Query = Promise<Record<string, string | string[] | undefined>>;

const one = <T extends string>(value: unknown, allowed: readonly T[]): T | undefined => allowed.find((a) => a === value);

export default async function Reports({ searchParams }: { searchParams: Query }) {
  const { d, viewer, reader } = await gate();
  if (!reader) return <Blocked email={viewer.email} />;
  const query = await searchParams;
  const known = await versions(d.db);
  const filter: Filter = {
    kind: one<Kind>(query.kind, KINDS),
    status: one<Status>(query.status, STATUSES),
    version: one(query.version, known),
  };
  const now = new Date();
  // uploads sweep too, but a quiet week must not keep address hashes longer
  await sweep(d.db, now);
  return <ListView rows={await listReports(d.db, filter)} versions={known} filter={filter} usage={await usage(d.db, now)} viewer={viewer} />;
}
