import { currentUser } from '@clerk/nextjs/server';
import { redirect } from 'next/navigation';
import { canRead, viewerFor, type Viewer } from './access';
import { deps, type Deps } from './deps';

async function verifiedEmail(): Promise<string | null> {
  const address = (await currentUser())?.primaryEmailAddress;
  // an address nobody proved to own must not open an invitation
  return address?.verification?.status === 'verified' ? address.emailAddress : null;
}

export async function currentViewer(d: Deps): Promise<Viewer | null> {
  return viewerFor(d.db, await verifiedEmail(), d.ownerEmail);
}

/** For pages: the services and the person, who is signed in but may still be uninvited. */
export async function gate(): Promise<{ d: Deps; viewer: Viewer; reader: boolean }> {
  const d = await deps();
  if (!d) throw new Error('The site is not set up: see web/feedback/README.md');
  const viewer = await currentViewer(d);
  if (!viewer) redirect('/sign-in');
  return { d, viewer, reader: canRead(viewer) };
}
