# In-game feedback reports: design

Status: approved by the owner on 2026-11-26.

## Intent

The game is in development. Testers report bugs and ask for changes while they play.
A report carries a screenshot and logs, so the owner can see what happened without asking again.
The game uploads the report to a website. The owner and invited testers sign in there with email.

## Goals

- A tester sends a report from inside the game in under one minute, with nothing to install.
- A report gives enough to repeat a bug: screenshot, log, game version, machine data, and the save.
- Only signed-in, invited people read reports.
- A report is not lost when the network or the server is down.

## Non-goals (first version)

- Comments, votes, assignment, and email notices about new reports.
- A public page or a public roadmap.
- A copy of reports to GitHub issues.
- Video recording.
- Detection of crashes that do not raise a Rust panic (driver abort, segmentation fault, forced kill).

## Anti-goals

- The game sends data without a press of a button.
- A secret key ships inside the game or enters the public repository.
- A report reaches the tester's own save slots or changes them.

## Constraints

Checked on 2026-11-26.

| Limit | Value | Source |
|---|---|---|
| Vercel Function request body | 4.5 MB | https://vercel.com/docs/functions/limitations |
| Blob storage, Hobby | 1 GB per month | https://vercel.com/docs/vercel-blob/usage-and-pricing |
| Blob advanced operations (upload, list), Hobby | 2,000 per month | same page |
| Blob over the limit, Hobby | no access until 30 days pass | same page |
| Clerk production instance | needs an owned domain and DNS records | https://clerk.com/docs/guides/development/deployment/production |
| Clerk email code sign-in | in the free plan | https://clerk.com/pricing |

The Hobby plan is for non-commercial use. A commercial release needs a paid plan.

## Approaches

| Way | Gain | Cost | Result |
|---|---|---|---|
| Zip file that the tester sends by hand | no server | sending depends on the tester | rejected |
| Own server on the home machine through Tailscale (Funnel or sharing) | own data | the owner did not want it | rejected |
| GitHub issues | good tracking | public repository; tester needs an account; no safe key in the game | rejected |
| Website on Vercel, direct upload from the game | nothing for the tester to install | free-plan limits; sign-in and upload protection to build | **chosen by the owner** |

For sign-in: a managed service (Clerk) sends the email codes and keeps sessions.
The other way is our own sign-in code plus a mail service. It adds security code that we would maintain.

## Game side

### Opening the form

- Key `F8` opens it. The action is rebindable and is named "Geri bildirim". The pause menu has a button too.
- An old `keys.cfg` that uses `F8` for another action keeps that choice.
- The game takes the screenshot before the form appears.
- While the form is open, keys type text and do not move the character.
- A solo world pauses. A shared world keeps running.
- On the start screen the form works without a save.

### Form

- Kind: "Hata" or "İstek".
- Text: required, 1 to 4,000 characters.
- Name: optional, up to 40 characters. The game remembers it.
- Three boxes, on by default: screenshot (with a small preview), log, save.
- A line says who reads the report: the developer and invited testers.
- Buttons: "Gönder", "Vazgeç".
- After "Gönder" the form closes at once. The upload runs in the background. A short message says "Gönderildi" or "Sonra gönderilecek".

### Contents

| Part | Content | Largest size |
|---|---|---|
| Fields | report id (random), kind, text, name, game version, operating system, graphics adapter, window size, interface zoom, seed, world size, day, mode (solo, host, guest), crash flag | 16 KB |
| `screenshot.png` | game window only, at most 1920 wide, made smaller until it fits | 2.5 MB |
| `log.txt` | end of the new game log, then the last 200 lines of `playlog.txt` | 300 KB |
| `save.alz` | the world as it is now, encoded in memory | 1 MB |

The whole request stays under 4 MB. A part that does not fit is left out, and the fields say so.

### Game log (new)

- The game has no log file today. Windows has no console, so messages are lost.
- `game.log` in the game's data folder takes the messages the game prints now, plus start, load, save, join, disconnect, and panics.
- It is capped at 1 MB. The previous run's log is kept as one old file.
- Before upload the game replaces the home folder path and the user name, and removes network addresses and room codes.

### Outbox

- A report is written to `reports/<id>/` before the upload starts.
- Success deletes the folder.
- No network, status 429, or status 5xx: the game tries again at the next start and after the next report.
- Other 4xx: the game does not try again and says that the server refused the report.
- At most 10 reports wait. When the box is full, the form says so and does not drop an old report.

### Crash

- A panic writes a crash file on every system (today only Windows).
- The next start asks: "Oyun geçen sefer çöktü. Rapor gönderilsin mi?" with "Gönder" and "Hayır".
- A crash report carries the crash text, the log, and the latest save of the slot in use. It has no screenshot.

### Address

- Built in: `https://feedback.alazlab.com`.
- `ALAZ_FEEDBACK_URL` overrides it for development. Only `https`, or `http` to this machine.

## Website

Next.js in `web/feedback/` of this repository. The interface is in Turkish.

### Who can do what

| Person | Can |
|---|---|
| Anyone with the game | send a report; read nothing |
| Signed in, email not invited | see only "Davetli değilsin" |
| Invited tester | read all reports, screenshots, logs, and saves |
| Owner (email from the `ADMIN_EMAIL` setting) | also change status, delete reports, add and remove invited emails |

Sign-in is a six-digit code sent to the email address (Clerk).
The site keeps its own list of invited emails, so no paid Clerk feature is needed.

### Pages

- Sign-in.
- Report list: newest first; filters for kind, status, and version; used storage and monthly report count.
- Report detail: screenshot, text, fields, log as plain text, save download.
- Status: "Yeni", "Bakılıyor", "Çözüldü", "Kapatıldı".
- Invited emails (owner only).

## Upload interface

`POST /api/reports`, `multipart/form-data`: `report` (JSON), and optional files `screenshot`, `log`, `save`.

| Status | Meaning |
|---|---|
| 201 | stored |
| 200 | this report id is already stored |
| 400 | a field or a file is not valid |
| 413 | too large |
| 429 | too many reports; `Retry-After` says when |
| 503 | storage or monthly budget is full |

Limits:
- One network address: 5 reports per hour, 20 per day. The server keeps a keyed hash of the address for two days, not the address.
- All senders: 100 reports per day and 500 per 30 days (three uploads each stay under 2,000 operations).
- Stored files: 800 MB in total.

## Data

- Postgres (Neon, from the Vercel Marketplace): reports, invited emails, upload counters.
- Private Blob store: the three files. The database keeps their paths, so the site never lists the store.
- Delete removes the files and the rows. There is no automatic deletion.

## Security and privacy

- Every page and file route checks sign-in and invitation on the server.
- The upload route is the only open route. It validates every field, checks the PNG and save headers, and ignores client file names.
- The site shows report text and logs as text, never as HTML. Files leave with `nosniff`; log and save leave as downloads.
- Queries are parameterized. Security headers include a content security policy.
- Secrets live only in Vercel settings. The owner's email is a setting, not code.
- The server logs no report text and no addresses.

Accepted risk: anyone who finds the address can send junk until a limit stops it.
Junk can use up the monthly budget. Real reports then wait in the testers' outboxes.

## Steps that need the owner

1. Approve the new Vercel project and its two stores.
2. Accept the Marketplace terms for Neon and Clerk.
3. Add the DNS records for `feedback.alazlab.com` and for Clerk in Cloudflare.
4. Set `ADMIN_EMAIL`, then invite testers.

Development and tests run before step 3, on a Clerk development instance and a preview address.

## Testing

- Game: tests first for text cleaning, size limits, the outbox states, the key migration, and the form; upload tests against a server on this machine.
- Website: tests first for validation, limits, and the access check for each role.
- Before release: one real report from a Linux build and one from a Windows build to the live site.

## Order of work

1. Website and upload interface.
2. Game side.

## Left to the builder

Module layout, the exact database tables, libraries for queries and styles, page design, and message wording beyond the texts above.
