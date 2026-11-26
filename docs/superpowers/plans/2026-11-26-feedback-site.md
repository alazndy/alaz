# Feedback Site Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A website in `web/feedback/` that accepts reports from the game and shows them to the owner and invited testers.

**Architecture:** Next.js App Router. All behaviour lives in `src/lib/` and takes its database, file store, and clock as arguments, so tests run it against an in-process Postgres (PGlite) and a memory store. Route files and pages only connect real services to that logic.

**Tech Stack:** Next.js 16.4.6, React 19.3.1, TypeScript 5.9.3, `@clerk/nextjs` 7.10.4, `@clerk/localizations` 4.26.0, `@vercel/blob` 2.9.0, `@neondatabase/serverless` 1.2.1, zod 4.6.7, vitest 5.1.1, `@electric-sql/pglite` 0.5.16, pnpm 10.

**Spec:** `docs/superpowers/specs/2026-11-26-feedback-reports-design.md`. This plan covers "Website" and "Upload interface". The game side is a second plan.

## Global Constraints

- Pin the versions above exactly (no `^`). Each is at least one week old.
- Sizes in bytes: screenshot 2_500_000; log 300_000; save 1_000_000; fields 20_000; whole request 4_000_000.
- Limits: 5 reports per hour and 20 per day per network address; 100 per day and 500 per 30 days for all senders; 800_000_000 stored bytes.
- Upload answers: 201 stored, 200 already stored, 400 not valid, 413 too large, 429 with `Retry-After`, 503 full or unavailable.
- The server keeps a keyed hash (HMAC-SHA-256) of the address for two days, never the address.
- The server logs no report text and no addresses.
- Every page, file route, and server action checks sign-in and invitation on the server.
- Report text and logs render as text. Files leave with `X-Content-Type-Options: nosniff`; log and save leave as attachments.
- SQL is parameterized. Secrets and the owner's email come from environment variables only.
- Interface text is Turkish. Status names: "Yeni", "Bakılıyor", "Çözüldü", "Kapatıldı". A signed-in person who is not invited sees "Davetli değilsin".
- No change under `crates/`.

## Review Focus

- Many uploads at once: the limits are never exceeded (Task 3).
- The same report id again after a lost answer: 200, one row, no extra files (Task 3).
- The file store or the database fails in the middle of an upload: 503, no row without files, no files without a row (Task 3).
- Report text or a log that contains HTML or script: shown as text (Task 5).
- An email that was removed from the list, or differs only in letter case: access follows the list on the next request (Task 4).

---

### Task 1: Project and field validation

**Files:**
- Create: `web/feedback/package.json`, `tsconfig.json`, `vitest.config.ts`, `.gitignore`, `src/lib/report.ts`
- Test: `web/feedback/tests/report.test.ts`

**Interfaces:**
- Produces: `SIZE` and `RATE` constant objects with the values above; `KINDS = ['bug','idea']`; `STATUSES = ['new','looking','done','closed']`; `FILE_NAMES = ['screenshot','log','save']`; types `Kind`, `Status`, `FileName`, `Fields`; `parseFields(json: string): Fields | null`; `checkFile(name: FileName, bytes: Uint8Array): boolean`.
- `Fields`: `id` (32 lowercase hex), `kind`, `text` (1..4000 characters after trim, counted as code points), `name?` (≤40), `game_version` (`major.minor.patch` with optional suffix, ≤32), `os` (≤64), `gpu?` (≤128), `window?` (≤32), `zoom?` (0.2..4), `seed?` (string of ≤20 digits), `world_size?` (integer 1..65536), `day?` (integer 0..1e9), `mode` (`solo|host|guest|menu`), `crash` (boolean), `omitted` (array of `FileName`, default empty). Unknown keys are dropped. Control characters other than newline and tab are removed from `text`, `name`, `os`, `gpu`, `window`.
- `checkFile`: screenshot = PNG signature, an `IHDR` chunk first, width 1..1920, height 1..4320, within size; log = valid UTF-8 without NUL, within size; save = starts with `ALAZ`, at least 16 bytes, within size.

- [ ] **Step 1: Write the failing tests**

```ts
const ok = { id: 'a'.repeat(32), kind: 'bug', text: 'Ağaç yok oldu', game_version: '0.6.0', os: 'linux x86_64', mode: 'solo', crash: false };
const parse = (over: object) => parseFields(JSON.stringify({ ...ok, ...over }));

test('accepts a minimal report and fills omitted', () => { expect(parse({})).toMatchObject({ ...ok, omitted: [] }); });
test('drops unknown keys', () => { expect(parse({ admin: true })).not.toHaveProperty('admin'); });
test.each([
  [{ id: 'A'.repeat(32) }], [{ id: '../x' }], [{ kind: 'other' }], [{ text: '   ' }], [{ text: 'x'.repeat(4001) }],
  [{ name: 'x'.repeat(41) }], [{ game_version: 'latest' }], [{ seed: '12a' }], [{ mode: 'x' }], [{ crash: 'no' }], [{ zoom: 9 }],
])('rejects %j', (over) => { expect(parse(over)).toBeNull(); });
test('counts characters, not UTF-16 units', () => { expect(parse({ text: '🪨'.repeat(4000) })).not.toBeNull(); });
test('removes control characters but keeps lines', () => { expect(parse({ text: 'a\u0000b\u001b\nc' })!.text).toBe('ab\nc'); });
test('rejects text that is not JSON or too long', () => { expect(parseFields('{')).toBeNull(); expect(parseFields(' '.repeat(20_001))).toBeNull(); });
test('screenshot needs a PNG header and a width up to 1920', () => {
  expect(checkFile('screenshot', png(1920, 1080))).toBe(true);
  expect(checkFile('screenshot', png(1921, 1080))).toBe(false);
  expect(checkFile('screenshot', new TextEncoder().encode('<svg/>'))).toBe(false);
});
test('log must be text and small', () => { /* 'merhaba' true; [0xff,0xfe] false; [0x61,0] false; 300_001 bytes false */ });
test('save needs the magic', () => { /* 'ALAZ' + 12 zero bytes true; 'XXXX…' false; 1_000_001 bytes false */ });
```
`png(w,h)` is a test helper in `tests/helpers.ts`: signature, then a 13-byte `IHDR` chunk with big-endian width and height.

- [ ] **Step 2:** `pnpm install` then `pnpm test`: FAIL, module missing.
- [ ] **Step 3:** Implement `src/lib/report.ts`.
- [ ] **Step 4:** `pnpm test`: PASS.
- [ ] **Step 5:** Commit `feat(web): feedback report fields and file checks`.

### Task 2: Database

**Files:**
- Create: `src/lib/db.ts`, `src/lib/store.ts`
- Test: `tests/db.test.ts`

**Interfaces:**
- Produces: `interface Db { query<T = Record<string, unknown>>(sql: string, params?: unknown[]): Promise<T[]> }`; `ensureSchema(db): Promise<void>` (idempotent, `CREATE TABLE IF NOT EXISTS` for `reports`, `members`, `uploads`).
- `src/lib/store.ts`: `type StoredFiles = Partial<Record<FileName, string>>`; `type ReportRow = Fields & { status: Status; created_at: Date; files: StoredFiles; bytes: number }`; `insertReport(db, fields, files, bytes, now): Promise<boolean>` (false when the id exists); `getReport(db, id): Promise<ReportRow|null>`; `listReports(db, filter: {kind?, status?, version?}): Promise<ReportRow[]>` newest first, at most 200; `versions(db): Promise<string[]>`; `setStatus(db, id, status)`; `deleteReport(db, id): Promise<string[]>` returns the stored paths; `usage(db, now): Promise<{bytes: number; last30Days: number}>`; `listMembers(db): Promise<string[]>`; `addMember(db, email)`, `removeMember(db, email)`, `isMember(db, email)`. Emails are stored and compared in lower case, trimmed.

- [ ] **Step 1: Write the failing tests** with a helper `testDb()` that wraps a new `PGlite()` and calls `ensureSchema` twice. Tests: insert then get returns the same fields, files and status `new`; second insert with the same id returns false and keeps the first text; list is newest first and each filter narrows it; a text of `'); DROP TABLE reports; --` is stored literally; `setStatus` shows in `getReport`; delete returns the paths and the report is gone; usage sums bytes and counts only the last 30 days of uploads; member add/remove ignores case and spaces.
- [ ] **Step 2:** `pnpm test`: FAIL.
- [ ] **Step 3:** Implement. `reports(id text primary key, created_at timestamptz, status text, fields jsonb, files jsonb, bytes int, kind text, version text)`; `members(email text primary key)`; `uploads(token text primary key, ip_hash text, at timestamptz)`.
- [ ] **Step 4:** `pnpm test`: PASS.
- [ ] **Step 5:** Commit `feat(web): report and member storage`.

### Task 3: Limits and upload

**Files:**
- Create: `src/lib/limits.ts`, `src/lib/files.ts`, `src/lib/ingest.ts`
- Test: `tests/ingest.test.ts`

**Interfaces:**
- `files.ts`: `interface FileStore { put(path: string, bytes: Uint8Array, contentType: string): Promise<string>; get(path: string): Promise<Uint8Array|null>; del(paths: string[]): Promise<void> }`; `memoryStore()` in `tests/helpers.ts`.
- `limits.ts`: `reserve(db, ipHash, now): Promise<{ok: true; token: string} | {ok: false; status: 429|503; retryAfter?: number}>` — inserts an `uploads` row first, then counts including it, and removes it when a limit is passed (so concurrent calls cannot pass a limit); `release(db, token)`; `sweep(db, now)` clears `ip_hash` older than 2 days and deletes rows older than 31 days. Per-address limits answer 429; the limits for all senders answer 503.
- `ingest.ts`: `handleUpload(req: Request, deps: {db: Db; files: FileStore; secret: string; now: () => Date}): Promise<Response>`. Order: content type is multipart and `Content-Length`, when present, ≤ `SIZE.body` (else 400/413) → parse form → `parseFields` and `checkFile` for each part (400; a file over its size 413; summed parts over `SIZE.body` 413) → existing id answers 200 → stored bytes plus new bytes over 800_000_000 answers 503 → `reserve` → put files under `reports/<id>/<name>.<png|txt|alz>` → `insertReport`; when it returns false delete the files just put and answer 200. Any storage or database error deletes the files already put and answers 503. JSON body `{id}` on 200/201, `{error}` otherwise.
- The sender address is `x-vercel-forwarded-for`, else `x-real-ip`, else `unknown`; first entry of a list.

- [ ] **Step 1: Write the failing tests.** Helper `upload(over, files, headers)` builds a `Request` with `FormData`. Tests: 201 stores row and the three files with their content; 200 for the same id again, one row, three files in the store; 400 for bad fields, bad PNG, a non-multipart body, an unknown extra part is ignored; 413 for a too large `Content-Length` and a too large screenshot; sixth upload in one hour from one address → 429 with `Retry-After` > 0, another address → 201, the same address one hour later → 201; 21st in a day → 429; 101st of all senders in a day → 503; 501st in 30 days → 503; stored bytes at the cap → 503 and nothing put; 20 concurrent uploads from one address → exactly at most 5 are 201 and the store holds files only for stored rows; a store that throws on the second put → 503, store empty, no row; a database that throws on the report insert → 503, store empty; no raw address appears in any `uploads` row; `sweep` clears the hash after 2 days.
- [ ] **Step 2:** `pnpm test`: FAIL.
- [ ] **Step 3:** Implement the three files. `handleUpload` calls `sweep` once per accepted upload.
- [ ] **Step 4:** `pnpm test`: PASS.
- [ ] **Step 5:** Commit `feat(web): upload with size and rate limits`.

### Task 4: Access, file download, and actions

**Files:**
- Create: `src/lib/access.ts`, `src/lib/serve.ts`, `src/lib/admin.ts`
- Test: `tests/access.test.ts`

**Interfaces:**
- `access.ts`: `type Role = 'owner'|'tester'|'none'`; `type Viewer = {email: string; role: Role}`; `viewerFor(db, email: string|null, ownerEmail: string|undefined): Promise<Viewer|null>` (null email → null; empty or missing owner setting matches nobody); `canRead(v)`, `canManage(v)`.
- `serve.ts`: `serveFile({db, files}, viewer: Viewer|null, id: string, name: string): Promise<Response>` — 401 not signed in, 403 role none, 404 unknown id, name, or missing file. Headers: `Content-Type` `image/png` | `text/plain; charset=utf-8` | `application/octet-stream`; `nosniff`; `Cache-Control: private, no-store`; `Content-Disposition` `inline` for screenshot, else `attachment; filename="<id>.<ext>"`; `Content-Security-Policy: default-src 'none'; sandbox`.
- `admin.ts`: `changeStatus(deps, viewer, id, status)`, `removeReport(deps, viewer, id)`, `invite(deps, viewer, email)`, `uninvite(deps, viewer, email)`; each throws `Forbidden` unless `canManage`; `invite` rejects a string that is not an email (zod) with `Invalid`; `changeStatus` rejects an unknown status with `Invalid`. `removeReport` deletes files then rows; when file deletion fails the row stays.

- [ ] **Step 1: Write the failing tests:** each role for `viewerFor` including upper-case email, removed member loses access on the next call, empty owner setting; every `serveFile` status and header; a tester gets `Forbidden` from all four actions and nothing changes; the owner can do all four; a failing `del` keeps the report.
- [ ] **Step 2:** `pnpm test`: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `pnpm test`: PASS.
- [ ] **Step 5:** Commit `feat(web): invited access, downloads, owner actions`.

### Task 5: Pages

**Files:**
- Create: `next.config.ts`, `src/proxy.ts`, `src/lib/deps.ts`, `src/lib/session.ts`, `src/views/{list,detail,members,blocked}.tsx`, `src/app/{layout.tsx,page.tsx,globals.css}`, `src/app/sign-in/[[...sign-in]]/page.tsx`, `src/app/reports/page.tsx`, `src/app/reports/[id]/page.tsx`, `src/app/reports/[id]/file/[name]/route.ts`, `src/app/members/page.tsx`, `src/app/actions.ts`, `src/app/api/reports/route.ts`
- Test: `tests/views.test.tsx`

**Interfaces:**
- `deps.ts`: `deps(): Promise<{db: Db; files: FileStore; secret: string; ownerEmail?: string} | null>` — null when a required setting is missing (the upload route then answers 503). Real adapters arrive in Task 6; this task defines the function over `adapters.ts`.
- `session.ts`: `currentViewer(): Promise<Viewer|null>` — Clerk `currentUser()`, only a primary email whose verification status is `verified`.
- Views are pure components: `ListView({rows, versions, filter, usage, viewer})`, `DetailView({row, log, viewer, actions})`, `MembersView({emails, actions})`, `Blocked({email})`. Pages load data, check `canRead`/`canManage`, and render a view. Not signed in → redirect to `/sign-in`. Role none → `Blocked`. Members page for a tester → 404.
- `proxy.ts`: `clerkMiddleware` with its content security policy option; the matcher excludes `/api/reports` and static files.
- `next.config.ts` headers for all routes: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `Permissions-Policy: camera=(), microphone=(), geolocation=()`, `Strict-Transport-Security: max-age=63072000`.

- [ ] **Step 1: Write the failing tests** with `renderToStaticMarkup`: a report text `<script>alert(1)</script>` and a log `<img src=x onerror=alert(1)>` appear escaped in `DetailView` and never as tags; a tester sees no status buttons and no delete button, the owner sees them; the list shows the four Turkish status names in its filter, used storage in MB and "x / 500"; `Blocked` shows "Davetli değilsin"; an omitted file is named as missing.
- [ ] **Step 2:** `pnpm test`: FAIL.
- [ ] **Step 3:** Implement views, pages, routes. Delete asks for confirmation with a second page step (`?confirm=delete`), no client script.
- [ ] **Step 4:** `pnpm test` and `pnpm typecheck`: PASS.
- [ ] **Step 5:** Commit `feat(web): sign-in, report pages, upload route`.

### Task 6: Real services, local mode, documents

**Files:**
- Create: `src/lib/adapters.ts`, `web/feedback/README.md`, `web/feedback/.env.example`
- Modify: `README.md`, `memory.md`
- Test: `tests/adapters.test.ts`

**Interfaces:**
- `adapters.ts`: `settings(env: Record<string,string|undefined>): {kind:'cloud', databaseUrl, secret, ownerEmail?} | {kind:'local', dir, secret, ownerEmail?} | null`. Cloud needs `DATABASE_URL` and `IP_HASH_SECRET` (≥32 characters). Local needs `FEEDBACK_LOCAL_DIR` and is refused when `NODE_ENV` is `production` or `VERCEL` is set. `neonDb(url): Db`, `blobStore(): FileStore` (private access), `localDb(dir)`, `diskStore(dir)` (rejects a path that leaves `dir`).

- [ ] **Step 1: Write the failing tests:** each `settings` outcome including local refused in production and on Vercel, a short secret refused; `diskStore` round trip, delete, and refusal of `../x`.
- [ ] **Step 2:** `pnpm test`: FAIL.
- [ ] **Step 3:** Implement. Document the environment variables and the owner steps in `web/feedback/README.md`.
- [ ] **Step 4:** `pnpm test`, `pnpm typecheck`, `pnpm audit --prod`: pass. `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=<syntactically valid dummy> CLERK_SECRET_KEY=<dummy> pnpm build`: succeeds.
- [ ] **Step 5:** Start `pnpm dev` in local mode and send one real upload with `curl`: 201, then 200 for the same id, then a 429 on the sixth new id. Stop the server.
- [ ] **Step 6:** Commit `feat(web): cloud adapters and local mode` and push `feat/feedback`. Do not merge. Do not create anything on Vercel, Clerk, or Neon: those are owner steps.
