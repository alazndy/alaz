import { SignIn } from '@clerk/nextjs';

export default function SignInPage() {
  return (
    <main className="center">
      {/* a tester signs in for the first time with an email that has no account yet */}
      <SignIn withSignUp fallbackRedirectUrl="/reports" signUpFallbackRedirectUrl="/reports" />
    </main>
  );
}
