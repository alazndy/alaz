import { trTR } from '@clerk/localizations';
import { ClerkProvider, Show, UserButton } from '@clerk/nextjs';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import './globals.css';

export const metadata: Metadata = { title: 'ALAZ geri bildirim', robots: { index: false, follow: false } };

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <ClerkProvider dynamic localization={trTR}>
      <html lang="tr">
        <body>
          <nav>
            <a href="/reports">ALAZ geri bildirim</a>
            <Show when="signed-in">
              <UserButton />
            </Show>
          </nav>
          {children}
        </body>
      </html>
    </ClerkProvider>
  );
}
