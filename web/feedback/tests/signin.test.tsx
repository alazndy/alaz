import type { ReactElement } from 'react';
import { expect, test } from 'vitest';
import SignInPage from '@/app/sign-in/[[...sign-in]]/page';

test('an invited person without an account can make one on the same screen', () => {
  const page = SignInPage() as ReactElement<{ children: ReactElement<{ withSignUp?: boolean; fallbackRedirectUrl?: string; signUpFallbackRedirectUrl?: string }> }>;
  expect(page.props.children.props).toMatchObject({ withSignUp: true, fallbackRedirectUrl: '/reports', signUpFallbackRedirectUrl: '/reports' });
});
