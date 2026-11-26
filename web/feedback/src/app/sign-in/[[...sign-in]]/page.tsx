import { SignIn } from '@clerk/nextjs';

export default function SignInPage() {
  return (
    <main className="center">
      <SignIn fallbackRedirectUrl="/reports" />
    </main>
  );
}
