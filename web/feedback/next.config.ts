import type { NextConfig } from 'next';

const headers = [
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'X-Frame-Options', value: 'DENY' },
  { key: 'Referrer-Policy', value: 'no-referrer' },
  { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' },
  { key: 'Strict-Transport-Security', value: 'max-age=63072000' },
];

const config: NextConfig = {
  poweredByHeader: false,
  // the local development database loads its own files from disk
  serverExternalPackages: ['@electric-sql/pglite'],
  turbopack: { root: import.meta.dirname },
  // the file route sets a stricter policy of its own; these add to every answer
  headers: async () => [{ source: '/:path*', headers }],
};

export default config;
