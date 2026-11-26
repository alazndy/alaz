import { notFound } from 'next/navigation';
import { inviteAction, uninviteAction } from '@/app/actions';
import { canManage } from '@/lib/access';
import { gate } from '@/lib/session';
import { listMembers } from '@/lib/store';
import { MembersView } from '@/views/members';

export default async function Members({ searchParams }: { searchParams: Promise<{ error?: string }> }) {
  const { d, viewer } = await gate();
  if (!canManage(viewer)) notFound();
  return (
    <MembersView
      emails={await listMembers(d.db)}
      error={(await searchParams).error === 'email'}
      actions={{ add: inviteAction, remove: uninviteAction }}
    />
  );
}
