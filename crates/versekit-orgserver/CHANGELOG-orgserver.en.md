# Changes to the VerseKit org server

The org server has its own version line, apart from the app (5.x) and from the standard (`versekit-org v1`). Git tags are named `orgserver-v<version>`, the image is at `ghcr.io/xharig/versekit-orgserver`. Deutsch: [CHANGELOG-orgserver.md](CHANGELOG-orgserver.md).

## 1.1.0

Members' portal with Discord login. The first version that comes out as a finished image; it holds everything of 1.0.0.

- Members' portal at `/portal`: members of the org's Discord server log in with Discord and see stock, blueprints, orders and deliveries in the browser.
- Roles member, logistics and lead: from the Discord roles the lead maps under **Settings**, or by hand under **Access**. "Blocked" keeps someone out and acts at once. Without a role a login waits for the lead.
- Logistics creates, edits and closes orders and confirms deliveries; the deliveries show who confirmed them.
- The admin password at `/admin` stays the emergency access for the setup and when Discord fails.
- Login of the administration: the form says it is valid for 10 minutes; an expired one simply shows a fresh one instead of an error. A hidden user field helps password managers to match the password. If a browser holds the form cookie back, the form counts all the same when the browser itself confirms that it comes from this page (once, 10 minutes). If it comes from elsewhere, the page names origin and address. Typed text stays visible in every field, also when a password manager fills it.
- Login with Firefox and other browsers over an http address in the home network: the pages now send `Referrer-Policy: same-origin` instead of `no-referrer`, so browsers name their own origin when they submit instead of `null`. If a browser sends `Origin: null` all the same, the form counts when it carries the token this server handed out or the cookie tied to it; a form that says it comes from another site (`Sec-Fetch-Site: cross-site` or `same-site`) or from a foreign origin stays refused. This holds for the forms after the login, too. The password field takes a system font for its dots.
- Secure by default: without the three Discord settings the portal is off; login with PKCE and a single-use `state` tied to the browser; sessions in memory only, 8 hours at most, over after 2 hours without a click; every form with CSRF protection. Of Discord only the id, the names and the role ids are kept.
- VerseKit connects unchanged with code and key; the open standard `versekit-org` v1 stays the same.
- New data format 3; older data move at the start after a backup. Version 1.0.0 no longer starts on these data afterwards (way back: the backup of the update).
- Guide: create the Discord application and map the roles, step by step.
- The image builds on fixed base images (Rust and distroless, each pinned to its digest): the same tag always gives the same base.

## 1.0.0

First version for self-hosting.

- Follows the open standard `versekit-org` v1: discovery, login by device code, renewal with rotation, revocation, stock, blueprints, orders with deliveries.
- Status page at `/` and confirmation page at `/device` in the look of VerseKit, German and English after the browser.
- Administration at `/admin`: members with rights and last contact, stock, blueprints, deliveries, create, edit and close orders.
- Secure by default: the admin password is created on the first start, without HTTPS only in an explicit test mode, a self-check at the start.
- Finished image for amd64 and arm64 with a build provenance, a hardened container, a template with Caddy for HTTPS.
- Data move to a new format by themselves at an update, after a backup.
- Checking tool `versekit-orgcheck` for own panels following the standard.
