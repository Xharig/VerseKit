[Deutsch](/tools/orga-standard/README.md) · **English**

# VerseKit org standard (versekit-org), version 1

With this open standard any Star Citizen org connects its own web panel to VerseKit, without VerseKit having to change. A member types the org's address into VerseKit under **Settings › Connect org**, confirms a code in the org's panel, done.

The exact description is [`v1/openapi.yaml`](/tools/orga-standard/v1/openapi.yaml). A template server to host yourself lies in [`crates/versekit-orgserver`](/crates/versekit-orgserver).

## What the org provides

1. **Discovery** at `https://<your-address>/.well-known/versekit-org.json`: name, tag and the addresses of the endpoints, one entry per major version under `versions`.
2. **Login by code** following RFC 8628 (Device Authorization Grant): VerseKit fetches a code, shows it and opens your confirmation page. The member signs in there (Discord, your login or a key, as you choose), sees the requested rights and confirms.
3. **Tokens**: access tokens (one hour at most), refresh tokens that change on every refresh, revocation following RFC 7009.
4. **API** with `Authorization: Bearer …` below `api_base`.

## Which data flow

| Right | VerseKit may |
|---|---|
| `stock:read` | read the stock the members share (`GET /stock`) |
| `stock:write` | replace the member's own shared stock (`PUT /stock/mine`) |
| `blueprints:read` | read who holds which blueprints (`GET /blueprints`) |
| `blueprints:write` | replace the member's own blueprints (`PUT /blueprints/mine`) |
| `orders:read` | read the org's orders (`GET /orders`) |
| `orders:write` | book deliveries to open orders (`POST /orders/{id}/deliveries`) |

The member decides on your confirmation page which rights to give. `GET /me` needs no right. Everything else stays on the member's computer.

## Rules for your side

- HTTPS only. Every address in the discovery document lies on the host the member types or below it (`api.orga.example.org` for `orga.example.org`). VerseKit refuses other hosts.
- Keep tokens, device codes and keys as hashes only. Codes run out after ten minutes at most.
- The confirmation page shows org, member and rights and lets the member untick rights. VerseKit only opens the bare address, never a link with the code built in.
- Too many requests: `429` with `Retry-After`; maintenance: `503` with `Retry-After`.
- API errors as `application/problem+json` with a `code` (see the OpenAPI file).
- Lists may come in pages: `next_cursor` in the answer, `?cursor=` in the next request.
- `PUT …/mine` replaces the whole list each time; deliveries carry an `Idempotency-Key`, a repeat books nothing twice.

## Versions

- Version 1 only grows compatibly: new optional fields, new endpoints, new rights. Unknown fields are passed over on both sides.
- Whatever would break older VerseKit releases becomes version 2 with its own `api_base`. During the change the discovery document names both versions; VerseKit takes the newest it knows.

## Connecting your own panel

Two ways, one standard: orgs with their own database and panel build the interface above into their panel, orgs without an own solution take the template server that comes along. VerseKit uses only what is in [`v1/openapi.yaml`](v1/openapi.yaml) and in the discovery document, never anything only the template server can do. Its administration at `/admin` is not part of the standard.

You do not have to offer everything. A panel that keeps only the stock (per member and place, with quantity and quality) names in `scopes_supported` only `stock:read` and, if members should share their stock from VerseKit, `stock:write`. VerseKit asks only for rights offered; blueprints and orders come later simply as more rights. Rights VerseKit does not know it passes over.

This is how you check your panel:

1. Set up a test environment with a test member; never against real org data.
2. Start the checking tool from the repository: `cargo run --release -p versekit-orgserver --bin versekit-orgcheck -- https://orga.example.org` (help with `--help`).
3. Twice a code appears: confirm it on your confirmation page with the test member, the first time the one right asked for, the second time all of them.
4. The tool checks the discovery, the login by code, every operation with its error codes, the limits of the rights, `429` with `Retry-After`, the refresh with rotation and the revocation. It ends with a list "green / RED — reason" and the code 0 when nothing is red.

The writing checks share a test entry as own stock and own blueprints of the test member for a moment and take it back; `--no-write` leaves them out. `--burst N` sets how many quick requests should set off the rate limit (default 300, `0` leaves the check out). `503` cannot be set off from outside and stays "skipped".

## Template server

`crates/versekit-orgserver` follows the standard and is small enough to read: members with a personal key in `config.json`, state in `state.json`, the confirmation page at `/device`. It speaks plain HTTP; HTTPS is the job of a reverse proxy in front (Caddy, Traefik, nginx).

The server has its own version line from 1.0.0, apart from the app and from the standard; changes are in [`CHANGELOG-orgserver.en.md`](/crates/versekit-orgserver/CHANGELOG-orgserver.en.md). From org server 1.1.0 the finished image is at `ghcr.io/xharig/versekit-orgserver` (amd64 and arm64, tags `:1.1.0`, `:1`, `:latest`), see the next section. To build it yourself: `docker build -f crates/versekit-orgserver/Dockerfile -t versekit-orgserver .` in the root folder of the repository, then put `image: versekit-orgserver` into the compose file. Without Docker: `cargo run --release -p versekit-orgserver -- --config config.json --state state.json --listen 127.0.0.1:8080`.

Environment variables come before `config.json`: `VERSEKIT_ORG_PUBLIC_URL`, `VERSEKIT_ORG_NAME`, `VERSEKIT_ORG_TAG`, `VERSEKIT_ORG_RATE_LIMIT` (requests per minute and login of a member before `429`, default 120), `VERSEKIT_ORG_POLL_INTERVAL` (seconds between two asks while the code is confirmed, default 5). Also `VERSEKIT_ORG_CONFIG`, `VERSEKIT_ORG_STATE` and `VERSEKIT_ORG_LISTEN` for the places.

## Self-hosting in 5 minutes

You need a machine with Docker (server, NAS, Raspberry Pi; amd64 or arm64) and a domain pointing to it.

1. **Get the files:** put [`docker-compose.yml`](/tools/orga-standard/docker-compose.yml) and [`config.example.json`](/crates/versekit-orgserver/config.example.json) into an empty folder and rename the second to `config.json`.
2. **Fill in:** in `config.json` an `id`, a name and an own key of at least 16 random characters per member. In `docker-compose.yml` your address, name and tag.
3. **Start:** `docker compose up -d`. The server listens only on `127.0.0.1:8080`. The state lies in the volume `orga-daten` and stays across restarts and updates.
4. **HTTPS in front**, easiest with Caddy, which fetches the certificate by itself. The whole `Caddyfile`:
   ```
   orga.example.org {
       reverse_proxy 127.0.0.1:8080
   }
   ```
   The same with Traefik or nginx: pass everything on to `127.0.0.1:8080`.
5. **Check:** `https://orga.example.org/.well-known/versekit-org.json` shows name and tag. Then enter the address in VerseKit under **Settings › Connect org** and confirm the code at `https://orga.example.org/device` with your own key. VerseKit shows "Connected to … as …".

Update: `docker compose pull && docker compose up -d`. The templates use `:1` (every update of line 1, never a breaking 2); a fixed version such as `:1.1.0` changes only when you change it in the compose file. Without an own reverse proxy take [`docker-compose.caddy.yml`](docker-compose.caddy.yml), which brings Caddy along. The detailed guide for newcomers, with NAS, Unraid, backup, keeping up to date and troubleshooting: [selbst-hosten.en.md](selbst-hosten.en.md).

## Administration

At `/admin` the org's leadership manages its server in the browser: connected members with their rights and last contact (disconnect by button), stock, blueprints and deliveries to search, and orders to create, edit and close. Each order shows the deliveries booked and the progress.

1. **Password:** on the first start the server creates a strong password and puts it into the data folder, readable for the owner only (`admin-password.txt`). The log names once where it lies, never the password itself. Show it: `docker compose exec orgserver versekit-orgserver --admin-password`. To set your own, put a file `.env` next to the compose file with `VERSEKIT_ORG_ADMIN_PASSWORD=` and at least 12 characters, or name a file with `VERSEKIT_ORG_ADMIN_PASSWORD_FILE` (a Docker secret for example). It is never in the image.
2. **Turn it off (only on purpose):** `VERSEKIT_ORG_ADMIN=off`. Then `/admin` says the administration is off, and the log warns at the start.
3. **Log in:** open `https://orga.example.org/admin` and type the password. The login lasts eight hours or until "Log out".

The card "Setup" of the administration shows the self-check at the start: password, HTTPS, rights on the data folder, clock and version. If something is red there, the public status page says only "Setup incomplete", without details.

Protection: the server holds the password only as a PBKDF2 hash. The session sits in a cookie with `HttpOnly`, `SameSite=Strict` and, under HTTPS, `Secure`. Every form carries a CSRF token and must come from the own address. After five wrong passwords the login waits a minute, twice as long each time after that, at most 15 minutes. The status page at `/` shows nothing of the administration.

Members and their keys stay in `config.json`. Whoever is missing there cannot log in; "Disconnect" only ends the current login.

## Members' portal (from 1.1.0)

At `/portal` members of the org's Discord server log in with Discord and see the same pages as the administration, as far as their role reaches: member (look), logistics (also orders and confirming deliveries), lead (everything, also access and settings). The roles come from the Discord roles or by hand. Without the three Discord settings the portal is off; the admin password at `/admin` stays the emergency access. The portal is not part of the standard. Setup step by step: [selbst-hosten.en.md](selbst-hosten.en.md#members-portal-with-discord-from-110).

## Testing

Set up a test environment with test accounts and enter its address in VerseKit. Nobody develops against real org data. VerseKit's tests run against the template server (`crates/versekit-core/tests/orga_standard.rs`) and show the whole flow.
