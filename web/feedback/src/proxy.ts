import { clerkMiddleware } from '@clerk/nextjs/server';

// Rights are checked in each page, route, and action; this only keeps the session and sets the content policy.
export default clerkMiddleware({ contentSecurityPolicy: { strict: true } });

export const config = {
  // the upload route takes reports from the game, which has no session
  matcher: ['/((?!api/reports|_next|favicon.ico).*)'],
};
