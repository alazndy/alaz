'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { changeStatus, Invalid, invite, removeReport, uninvite } from '@/lib/admin';
import { gate } from '@/lib/session';

// Anyone can call a server action by hand, so each one looks up the person again; `admin` refuses all but the owner.

const text = (form: FormData, name: string) => String(form.get(name) ?? '');

export async function setStatusAction(id: string, form: FormData) {
  const { d, viewer } = await gate();
  await changeStatus(d, viewer, id, text(form, 'status'));
  revalidatePath(`/reports/${id}`);
}

export async function removeReportAction(id: string) {
  const { d, viewer } = await gate();
  await removeReport(d, viewer, id);
  revalidatePath('/reports');
  redirect('/reports');
}

export async function inviteAction(form: FormData) {
  const { d, viewer } = await gate();
  try {
    await invite(d, viewer, text(form, 'email'));
  } catch (error) {
    if (error instanceof Invalid) redirect('/members?error=email');
    throw error;
  }
  redirect('/members');
}

export async function uninviteAction(form: FormData) {
  const { d, viewer } = await gate();
  await uninvite(d, viewer, text(form, 'email'));
  revalidatePath('/members');
}
