# Exchange API v1 — error codes

Every error of the exchange API is an RFC 9457 `application/problem+json` document
([`problem.schema.json`](schemas/)) whose `code` is one of the codes below (`REQ-XCH-025`). A code
is never reused or repurposed; new codes may appear within v1, and a client treats an unknown code
by its HTTP status.

The `detail` is a short English sentence fixed per code and situation. It never echoes the request,
and a refusal raised by the Basetool behind the gateway arrives with its code's own sentence, not
the Basetool's internal text, whatever the `Accept-Language`. Decide by `code`; show `detail` at
most as a hint.

The gateway's gates count their refusals on the metric `basetool_ingest_exchange_refused_total`,
with the code in snake case as its `reason` label; the answers of the routes themselves, those
given before the token is checked and the DPoP nonce challenge are not counted there. The
**per-op** reasons never arrive as a problem: they appear in a change result's `results[].reason`
for an op that was not applied.

The Basetool behind the gateway checks the switch, the registry, the revocations and the
capabilities of every request again. The gateway reads the registry through a cache of up to
five seconds, so right after a change the Basetool may be the one that refuses. It then answers
with the **same** code, status, `Retry-After` and `detail` the gateway would have — the codes
raised by *gateway, backend* below, `SCHEMA_INVALID` aside — and such a refusal is not counted on
the gateway's metric. A client never sees one situation under two
codes, whichever side refuses it.

## Request errors

| Code | HTTP | Raised by | Meaning | Client action |
| --- | --- | --- | --- | --- |
| `CLIENT_NOT_ALLOWED` | 403 | gateway, backend | The token's client is not in the registry. | Stop; the client is not approved. |
| `CLIENT_SUSPENDED` | 403 | gateway, backend | The client is suspended in the registry. | Stop and tell the member. Nothing tells a client when the suspension ends, so do not retry on a timer: try again at the client's next start or when the member asks. |
| `CLIENT_REVOKED` | 401 | gateway, backend | The member disconnected this client after this connection was made: an offline token issued before the disconnect, or a token without `offline_access` whose sign-in (`auth_time`) came before it. | Discard tokens; start a new device login only when the member asks. |
| `INSTALLATION_REVOKED` | 401 | gateway, backend | The member disconnected this installation; its DPoP key is refused for good. | Discard tokens **and** the DPoP key; reconnecting needs a new key. |
| `CLIENT_VERSION_UNSUPPORTED` | 403 | gateway | The `User-Agent` version is below the client's minimum. | Ask the member to update. |
| `EXCHANGE_DISABLED` | 503 | gateway, backend | The exchange is switched off globally. `Retry-After: 30`. | Wait at least `Retry-After`, then retry. |
| `REGISTRY_UNAVAILABLE` | 503 | gateway, backend | The gateway cannot read the client registry or the revocations, or the Basetool cannot read the revocations, and fails closed. `Retry-After: 30`. | Wait at least `Retry-After`, then retry. |
| `EXCHANGE_BUDGET_EXHAUSTED` | 503 | gateway | A Redis byte budget of the exchange is full: the member's, the client's or the total. `Retry-After` is the seconds until enough of it expires, at most 3600. The refused write does not count against the daily write quota. | Wait at least `Retry-After`, then retry the same request under the same key. Send only what changed: a budget fills with the answers to full-list pushes and to change sets that keep conflicting. |
| `RELAY_BUSY` | 503 | gateway | A change set of more than 100 ops, while the gateway already relays as many of those as it admits at once (four). Nothing was written, and the set does not count against the daily write quota. `Retry-After: 10`. | Wait at least `Retry-After`, then retry the same request under the same key; or split it into change sets of at most 100 ops. |
| `SCOPE_MISSING` | 403 | gateway, backend | The route's capability is not in the token or not granted to the client. A missing consent looks the same. | Start a device login requesting the scope, if the member wants it. |
| `UNAUTHENTICATED` | 401 | gateway | The token is missing, invalid, expired, or not issued for this gateway. | Refresh the token once and retry. Only when the refresh answers `invalid_grant` is the connection over: delete the refresh token and start a device login when the member asks. A freshly refreshed token that is refused again is not retried in a loop. |
| `DPOP_REQUIRED` | 401 | gateway | The request carries no DPoP proof or an unbound token. | Send `Authorization: DPoP` with a proof. |
| `DPOP_INVALID` | 401 | gateway | The proof is invalid, replayed, for another key, or lacks the server nonce. | Fix the proof; on a nonce challenge retry once with the `DPoP-Nonce`. |
| `DPOP_PROOF_LIMIT` | 429 | gateway | The member already holds its cap of live DPoP proofs, over all of its clients ([live proofs](authentication.md#live-proofs-per-member)). `Retry-After` is the seconds until the member's earliest live proof no longer counts. | Wait at least `Retry-After`, then retry with a new proof; slow the member's requests down. |
| `TERMS_NOT_ACCEPTED` | 403 | backend | The member has not accepted the current terms. | Ask the member to open the Basetool and accept. |
| `PENDING_APPROVAL` | 403 | backend | The member's registration awaits approval. | Stop; nothing to sync yet. |
| `NO_ROLE` | 403 | backend | The member holds no role. | Stop and tell the member. |
| `ACTING_MEMBER_REFUSED` | 403 | backend | The relay refused the member (unknown, disabled or deleted). | Stop and tell the member. |
| `NOT_PERMITTED` | 403 | backend | The member may not do this. | Stop; do not retry. |
| `SCHEMA_INVALID` | 400 | gateway, backend | The body or a query parameter does not match the v1 contract; `errors[]` points at the fields, a parameter as `/<name>`. A body that is not a JSON document has one error at the pointer `""` and is never cached for its `Idempotency-Key`; a query parameter without a name has one at `/`. From the backend, without `errors[]`: the content is malformed although it matches the schema, such as a refinery draft of an unsupported panel type. | Fix the request. |
| `BATCH_TOO_LARGE` | 413 | gateway | A change set holds more than 500 ops, or is too large to hold for the member's confirmation. | Split the batch. |
| `PAYLOAD_TOO_LARGE` | 413 | gateway | The body exceeds the size cap, or the draft built from it is too large to hand off. | Split or shrink the request. |
| `IDEMPOTENCY_KEY_MISSING` | 400 | gateway | A write carries no `Idempotency-Key`, or none of 8 to 128 characters of `[A-Za-z0-9._~-]`. | Send a fresh key per logical write. |
| `IDEMPOTENCY_KEY_REUSED` | 422 | gateway | The key was used with a different request: another route or another body. | Use a fresh key. |
| `IDEMPOTENCY_IN_PROGRESS` | 409 | gateway | The same key is still being processed. | Retry the same request after a short wait. |
| `CURSOR_EXPIRED` | 410 | backend, gateway | The cursor is older than the tombstones, or not one the server issued. | Reconcile a full snapshot against the last baseline — not add-only. |
| `VERSION_CONFLICT` | 409 | backend | A ship's `version` or a lot's `expectedQuantity` no longer matches. | Pull, merge, retry. |
| `MASS_CHANGE_CONFIRMATION_REQUIRED` | 409 | backend | The batch exceeds the mass-change guard; it is staged. | Show the member `confirmationUrl`; do not retry the batch. |
| `RATE_LIMITED` | 429 | gateway | A limit per minute or per hour is exhausted: per client and member, per client, per source IP address, or the hourly account check ([limits](sync-guide.md#rate-limits-quota-and-back-off)). | Honour `Retry-After`. |
| `QUOTA_EXCEEDED` | 429 | gateway | The daily write quota is exhausted. | Retry after `Retry-After`, the next day at the latest. |
| `BACKEND_RELAY_FAILED` | 502 | gateway | The backend did not answer usably: an error, a refusal the contract does not name, or an answer that breaks the v1 schema. | Back off; retry with the same key. |
| `SERVICE_UNAVAILABLE` | 503 | gateway | Temporarily unavailable: a store the exchange needs cannot be reached (`Retry-After: 60`), the daily write quota cannot be counted (`Retry-After: 30`), the identity provider cannot be reached to check the token (`Retry-After: 5`), or all members together hold the gateway's cap of live DPoP proofs ([live proofs](authentication.md#live-proofs-per-member); `Retry-After` is the seconds until the earliest of them no longer counts). | Wait at least `Retry-After`, then retry the same request under the same key, with a new DPoP proof. |
| `NOT_FOUND` | 404 | gateway | The exchange has no such route or method, or the requested document, such as a schema name, does not exist. | Check the path, the method and the name. |
| `UNSUPPORTED_MEDIA_TYPE` | 415 | gateway | A request body is not sent as `Content-Type: application/json`. Never cached for its `Idempotency-Key`. | Send the body as `application/json`. |
| `INTERNAL_ERROR` | 500 | gateway | An unexpected failure of the gateway, the generic fallback. Never cached. | Back off and retry under the same key; report the `correlationId` if it persists. |
| `LEGACY_ENDPOINT_GONE` | 410 | gateway | A legacy `/v1/*` extractor endpoint after the go-live. | Update the client. |

## Per-op reasons in a change result

| Reason | Meaning | Client action |
| --- | --- | --- |
| `UNMATCHED` | The reference resolves to nothing. | Show it; offer `catalog/resolve`. |
| `AMBIGUOUS` | The reference resolves to several entries. | Ask the member to pick; send `bt`. |
| `DEFAULT_NOT_REMOVABLE` | A default-granted blueprint cannot be removed. | Keep it. |
| `STOCK_EARMARKED` | The lot's stock is reserved for a job order or mission. | Ask the member to release the reservation in the web. |
| `STOLEN_MARKING_DISABLED` | The Basetool does not yet keep stolen stock apart. | Keep the lot local until the server supports it. |
| `REMOVED_ELSEWHERE` | The entry has a live tombstone from another channel or installation; for a ship, the named `shipId` was removed there. | Ask the member; resend with `override: true` only if they agree. |
| `UNIT_MISMATCH` | The quantity's unit does not match the material's. | Fix the unit. |
| `LOCATION_UNKNOWN` | The place has no warehouse location. | Offer a place from `catalog/locations`. |
| `LINK_TARGET_TAKEN` | The server ship is already linked to another external id of this installation. | Pull and re-link. |
| `VERSION_CONFLICT` | As above, for this op only; also an `upsert` without `shipId` for an id this installation already linked. | Pull, merge, retry this op. |

## Warnings

A change result or resolve result may carry `warnings[]` with a JSON Pointer and a code. v1 defines
`UNKNOWN_FIELD` (the server ignored a field it does not know) and `LOC_KEY_UNRESOLVED` (the
reference's key fields, its `locKey` included, resolved to no single catalogue entry; the name was
tried next, when one was sent).

## The problem document

| Field | Sent | Meaning |
| --- | --- | --- |
| `status` | always | The HTTP status. |
| `code` | always | The code from the tables above. Decide by it. |
| `title` | always | A short English title, usually the status's reason phrase. |
| `detail` | always | The fixed English sentence for the code; a hint at most. |
| `correlationId` | always | The request's id, the same value as the `X-Correlation-Id` response header. |
| `instance` | from the routes, not from the gateway's filters | The request path. |
| `errors` | with `SCHEMA_INVALID` from the gateway | Up to 50 `{pointer, message}`; the message names the violated keyword, never a value you sent. |
| `confirmationUrl` | with `MASS_CHANGE_CONFIRMATION_REQUIRED` | Where the member confirms the held batch ([sync guide](sync-guide.md#the-mass-change-guard)). |
| `type` | never | Absent, which RFC 9457 reads as `about:blank`. |
| `retryAfterSeconds` | never | Reserved; the delay is in the `Retry-After` header. |

Every answer, a success included, carries `X-Correlation-Id`. Put it into a problem report or a log
line, so the Basetool's maintainers can find the request; it identifies nothing but the request. A
client may send its own `X-Correlation-Id` of 1 to 128 characters of `A–Z`, `a–z`, `0–9`, `.`, `_`
and `-`; the gateway keeps it, and replaces any other value with a random UUID.
