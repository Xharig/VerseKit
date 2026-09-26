# Exchange API v1 — error codes

Every error of the exchange API is an RFC 9457 `application/problem+json` document
([`problem.schema.json`](https://ingest.profit-base.online/exchange/v1/schemas/problem.schema.json))
whose `code` is one of the codes below (`REQ-XCH-025`). A code is never reused or repurposed; new
codes may appear within v1, and a client treats an unknown code by its HTTP status.

The **gateway** codes are the `reason` label values of the gateway's exchange metrics, in snake
case. The **per-op** reasons never arrive as a problem: they appear in a change result's
`results[].reason` for an op that was not applied.

## Request errors

| Code | HTTP | Raised by | Meaning | Client action |
| --- | --- | --- | --- | --- |
| `CLIENT_NOT_ALLOWED` | 403 | gateway | The token's client is not in the registry. | Stop; the client is not approved. |
| `CLIENT_SUSPENDED` | 403 | gateway | The client is suspended in the registry. | Stop and tell the member; retry after the maintainer resolved it. |
| `CLIENT_REVOKED` | 401 | gateway | The member disconnected this client after the token was issued. | Discard tokens; start a new device login only when the member asks. |
| `INSTALLATION_REVOKED` | 401 | gateway | The member disconnected this installation; its DPoP key is refused for good. | Discard tokens **and** the DPoP key; reconnecting needs a new key. |
| `CLIENT_VERSION_UNSUPPORTED` | 403 | gateway | The `User-Agent` version is below the client's minimum. | Ask the member to update. |
| `EXCHANGE_DISABLED` | 503 | gateway | The exchange is switched off globally. | Back off; retry later. |
| `REGISTRY_UNAVAILABLE` | 503 | gateway | The gateway cannot read the client registry and fails closed. | Back off; retry later. |
| `EXCHANGE_BUDGET_EXHAUSTED` | 503 | gateway | A Redis byte budget of the exchange is full. | Back off; honour `Retry-After`. |
| `SCOPE_MISSING` | 403 | gateway | The route's capability is not in the token or not granted to the client. A missing consent looks the same. | Start a device login requesting the scope, if the member wants it. |
| `DPOP_REQUIRED` | 401 | gateway | The request carries no DPoP proof or an unbound token. | Send `Authorization: DPoP` with a proof. |
| `DPOP_INVALID` | 401 | gateway | The proof is invalid, replayed, for another key, or lacks the server nonce. | Fix the proof; on a nonce challenge retry once with the `DPoP-Nonce`. |
| `TERMS_NOT_ACCEPTED` | 403 | backend | The member has not accepted the current terms. | Ask the member to open the Basetool and accept. |
| `PENDING_APPROVAL` | 403 | backend | The member's registration awaits approval. | Stop; nothing to sync yet. |
| `NO_ROLE` | 403 | backend | The member holds no role. | Stop and tell the member. |
| `ACTING_MEMBER_REFUSED` | 403 | backend | The relay refused the member (unknown, disabled or deleted). | Stop and tell the member. |
| `NOT_PERMITTED` | 403 | backend | The member may not do this. | Stop; do not retry. |
| `SCHEMA_INVALID` | 400 | gateway | The body does not match the v1 schema; `errors[]` points at the fields. | Fix the request. |
| `BATCH_TOO_LARGE` | 413 | gateway | A change set holds more than 500 ops. | Split the batch. |
| `PAYLOAD_TOO_LARGE` | 413 | gateway | The body exceeds the size cap. | Split or shrink the request. |
| `IDEMPOTENCY_KEY_MISSING` | 400 | gateway | A write carries no `Idempotency-Key`. | Send a fresh key per logical write. |
| `IDEMPOTENCY_KEY_REUSED` | 422 | gateway | The key was used with a different body. | Use a fresh key. |
| `IDEMPOTENCY_IN_PROGRESS` | 409 | gateway | The same key is still being processed. | Retry the same request after a short wait. |
| `CURSOR_EXPIRED` | 410 | backend | The cursor is older than the tombstones. | Reconcile a full snapshot against the last baseline — not add-only. |
| `VERSION_CONFLICT` | 409 | backend | A ship's `version` or a lot's `expectedQuantity` no longer matches. | Pull, merge, retry. |
| `MASS_CHANGE_CONFIRMATION_REQUIRED` | 409 | backend | The batch exceeds the mass-change guard; it is staged. | Show the member `confirmationUrl`; do not retry the batch. |
| `RATE_LIMITED` | 429 | gateway | A per-minute limit is exhausted. | Honour `Retry-After`. |
| `QUOTA_EXCEEDED` | 429 | gateway | The daily write quota is exhausted. | Retry after `Retry-After`, the next day at the latest. |
| `BACKEND_RELAY_FAILED` | 502 | gateway | The backend did not answer usably. | Back off; retry with the same key. |
| `SERVICE_UNAVAILABLE` | 503 | gateway | Temporarily unavailable. | Back off; retry with the same key. |
| `LEGACY_ENDPOINT_GONE` | 410 | gateway | A legacy `/v1/*` extractor endpoint after the go-live. | Update the client. |

## Per-op reasons in a change result

| Reason | Meaning | Client action |
| --- | --- | --- |
| `UNMATCHED` | The reference resolves to nothing. | Show it; offer `catalog/resolve`. |
| `AMBIGUOUS` | The reference resolves to several entries. | Ask the member to pick; send `bt`. |
| `DEFAULT_NOT_REMOVABLE` | A default-granted blueprint cannot be removed. | Keep it. |
| `REMOVED_ELSEWHERE` | The entry has a live tombstone from another channel or installation. | Ask the member; resend with `override: true` only if they agree. |
| `UNIT_MISMATCH` | The quantity's unit does not match the material's. | Fix the unit. |
| `LOCATION_UNKNOWN` | The place has no Lager location. | Offer a place from `catalog/locations`. |
| `LINK_TARGET_TAKEN` | The server ship is already linked to another external id. | Pull and re-link. |
| `VERSION_CONFLICT` | As above, for this op only. | Pull, merge, retry this op. |

## Warnings

A change result or resolve result may carry `warnings[]` with a JSON Pointer and a code. v1 defines
`UNKNOWN_FIELD` (the server ignored a field it does not know) and `LOC_KEY_UNRESOLVED` (the
catalogue carries no name key yet; the name was used instead).
