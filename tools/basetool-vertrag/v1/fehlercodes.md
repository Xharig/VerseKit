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
| `ORIGIN_NOT_ALLOWED` | 403 | gateway | The request's `Origin` is a browser origin this client is not approved for: another client's extension id, a Firefox extension for a client without a Firefox build, or a web page. | Stop; send requests only from the extension builds named in the approval. |
| `CLIENT_REVOKED` | 401 | gateway, backend | The member disconnected this client after this connection was made: an offline token issued before the disconnect, or a token without `offline_access` whose sign-in (`auth_time`) came before it. | Discard tokens; start a new device login only when the member asks. |
| `INSTALLATION_REVOKED` | 401 | gateway, backend | The member disconnected this installation; its DPoP key is refused for good. | Discard tokens **and** the DPoP key; reconnecting needs a new key. |
| `CLIENT_VERSION_UNSUPPORTED` | 403 | gateway | The `User-Agent` version is below the client's minimum. | Ask the member to update. |
| `BANK_EXCHANGE_DISABLED` | 503 | gateway, backend | The bank is switched off for connected applications while the exchange is on: every path below `/exchange/v1/bank` answers this, whether a route serves it or not. `Retry-After: 300`. | Pause the bank area only and keep syncing every other area; wait at least `Retry-After`, then retry the bank. |
| `EXCHANGE_DISABLED` | 503 | gateway, backend | The exchange is switched off globally. `Retry-After: 30`. | Wait at least `Retry-After`, then retry. |
| `REGISTRY_UNAVAILABLE` | 503 | gateway, backend | The gateway cannot read the client registry or the revocations, or the Basetool cannot read the revocations, and fails closed. `Retry-After: 30`. | Wait at least `Retry-After`, then retry. |
| `EXCHANGE_BUDGET_EXHAUSTED` | 503 | gateway | A Redis byte budget of the exchange is full: the member's, the client's or the total. `Retry-After` is the seconds until enough of what it holds now expires, never shorter, so it can be hours. The refused write does not count against the daily write quota. | Wait at least `Retry-After`, then retry the same request under the same key. Send only what changed: a budget fills with the answers to full-list pushes and to change sets that keep conflicting. |
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
| `VERSION_CONFLICT` | 409 | backend | A ship's, org-stock entry's or bank request's `version`, or a lot's `expectedQuantity`, no longer matches. | Pull, merge, retry. |
| `BANK_OPEN_REQUEST_CAP` | 409 | backend | The member already holds 20 pending bank requests, from any channel; nothing was raised. | Wait for the bank's decisions or let the member cancel one; do not retry on a timer. |
| `BANK_JUSTIFICATION_REQUIRED` | 409 | backend | A bank withdrawal or transfer from this account needs a justification. | Ask the member for one and resend with a new key. |
| `BANK_SELF_TRANSFER` | 409 | backend | A bank transfer names its source as its destination. | Fix the destination. |
| `BANK_REQUEST_NOT_PENDING` | 409 | backend | The bank request was decided or cancelled already. | Read the own requests again; do not retry. |
| `BANK_REQUEST_ALREADY_APPROVED` | 409 | backend | The bank request was approved and can no longer be corrected. | Cancel it and raise a new one, if the member wants. |
| `PESSIMISTIC_LOCK` | 409 | backend | Another write held an entry the change set needs, so the set could not start; nothing was applied. | Retry the same change set after a short wait. |
| `MASS_CHANGE_CONFIRMATION_REQUIRED` | 409 | backend | The batch exceeds the mass-change guard; it is staged. An identical resend while it waits gets the same `confirmationUrl`. | Show the member `confirmationUrl`; do not retry the batch. |
| `RELOCATION_PENDING` | 409 | backend | A patch change asks the member for a new home location, and the change set writes a stock lot or ship from before the patch's live time whose holder has not chosen yet: the member's own, or a pool lot another member holds. Nothing was written. Reads and new stock still work. | Pause the stock and ship sync, keep the local changes and ask the member to choose the home location in the Basetool; then pull and retry ([patch changes](sync-guide.md#patch-changes)). |
| `HOME_LOCATION_NOT_ASKED` | 409 | backend | A home-location choice while no open phase asks the member: none is open, an admin ended it, or the account was created after the live time. Nothing moved. | Read the [patch change](resources/patch-change.md) again; do not retry. |
| `HOME_LOCATION_PHASE_CHANGED` | 409 | backend | A home-location choice names another phase than the open one: a later patch change asked again since the client read the patch change. Nothing moved. | Read the [patch change](resources/patch-change.md) again and let the member choose from the new prompt. |
| `HOME_LOCATION_ALREADY_CHOSEN` | 409 | backend | The member chose the home location in this phase already, in the Basetool or through any client; the choice is final. | Read the [patch change](resources/patch-change.md) again and take the server's result; do not retry. |
| `HOME_LOCATION_NOT_OFFERED` | 400 | backend | The chosen location is not one of the home locations the patch-change read offers. Nothing moved. | Read the offered locations again and let the member pick one. |
| `RATE_LIMITED` | 429 | gateway | A limit per minute or per hour is exhausted: per client and member, per client, per source IP address, or the hourly account check ([limits](sync-guide.md#rate-limits-quota-and-back-off)). | Honour `Retry-After`. |
| `QUOTA_EXCEEDED` | 429 | gateway | The daily write quota is exhausted; an org-stock change set counts each of its ops and is refused as a whole when they do not all fit. | Retry after `Retry-After`, the next day at the latest. |
| `BACKEND_RELAY_FAILED` | 502 | gateway | The backend did not answer usably: an error, a refusal the contract does not name, or an answer that breaks the v1 schema. | Back off; retry with the same key. |
| `SERVICE_UNAVAILABLE` | 503 | gateway | Temporarily unavailable: a store the exchange needs cannot be reached (`Retry-After: 60`), the daily write quota cannot be counted (`Retry-After: 30`), the identity provider cannot be reached to check the token (`Retry-After: 5`), or all members together hold the gateway's cap of live DPoP proofs ([live proofs](authentication.md#live-proofs-per-member); `Retry-After` is the seconds until the earliest of them no longer counts). | Wait at least `Retry-After`, then retry the same request under the same key, with a new DPoP proof. |
| `NOT_BANK_STAFF` | 403 | backend | The bank-staff queue was asked for by a member who holds no bank role and no account grant, and for whom no request waits for approval. Decided anew on every call. | Hide the queue; ask again later, a grant or a request to approve can come at any time. |
| `NOT_FOUND` | 404 | gateway, backend | The exchange has no such route or method, or the requested document, such as a schema name, does not exist. On a bank route: the account does not exist, is closed or the member may not view it, or a request to correct or cancel is not the member's, which answer alike. | Check the path, the method and the name; for a bank account, drop it from what you show. |
| `UNSUPPORTED_MEDIA_TYPE` | 415 | gateway | A request body is not sent as `Content-Type: application/json`. Never cached for its `Idempotency-Key`. | Send the body as `application/json`. |
| `INTERNAL_ERROR` | 500 | gateway | An unexpected failure of the gateway, the generic fallback. Never cached. | Back off and retry under the same key; report the `correlationId` if it persists. |
| `LEGACY_ENDPOINT_GONE` | 410 | gateway | *Retired 2026-09-28, never reused:* answered a legacy `/v1/*` extractor endpoint after the go-live; those routes are removed, and a request there now gets the gateway's answer for an unknown path. | Update the client. |

## Per-op reasons in a change result

| Reason | Meaning | Client action |
| --- | --- | --- |
| `UNMATCHED` | The reference resolves to nothing. | Show it; offer `catalog/resolve`. |
| `AMBIGUOUS` | The reference resolves to several entries. | Ask the member to pick; send `guid` for a blueprint, else `bt`. |
| `DEFAULT_NOT_REMOVABLE` | A default-granted blueprint cannot be removed. | Keep it. |
| `STOCK_EARMARKED` | The lot's stock, the stock a `share` or `unshare` would move, or the part of an org-stock `book-out` its earmark lines do not cover, is reserved for a job order, mission or donation. | Ask the member to release the reservation in the web, or name the earmarks to take from. |
| `STOLEN_MARKING_DISABLED` | The Basetool does not yet keep stolen stock apart. | Keep the lot local until the server supports it. |
| `REMOVED_ELSEWHERE` | The entry has a live tombstone from another channel or installation; for a ship, the named `shipId` was removed there. A lot or ship a patch change moved or wiped is refused whatever the `expectedQuantity`, and an org-stock op on an entry a patch change wiped is refused the same way; the result item names the patch change in `patchChange` ([patch change](resources/patch-change.md#pushing-against-what-a-patch-change-removed)). | Ask the member; resend with `override: true` only if they agree. For a patch change, drop the pending change and pull instead. |
| `UNIT_MISMATCH` | The quantity's unit does not match the material's; also a refinery store line's. | Fix the unit. |
| `LOCATION_UNKNOWN` | The place has no warehouse location; for a refinery order, it has no location. | Offer a place from `catalog/locations`. |
| `LINK_TARGET_TAKEN` | The server ship is already linked to another external id of this installation. | Pull and re-link. |
| `ORIGIN_LOCKED` | A ship `upsert` sends another `origin` than `PLEDGE` for a ship a hangar import or a pledge id locked to `PLEDGE`. | Take `PLEDGE` from the feed; resend without `origin` or with `PLEDGE`. |
| `VERSION_CONFLICT` | As above, for this op only; also an `upsert` without `shipId` for an id this installation already linked, and a refinery-order op whose counter of a section it touches is stale or missing ([counters](resources/refinery-orders.md#counters-in-a-change-set)). When the lot's, the ship's or the org-stock entry's last change was a patch change's, the result item names it in `patchChange` ([patch change](resources/patch-change.md#pushing-against-what-a-patch-change-removed)). | Pull, merge, retry this op. |
| `NOT_PERMITTED` | An org-stock op on a pool whose unit the member is not a Logistician of, a `move-holder` into such a pool or to the entry's own holder in another pool, or an op on an entry that is unknown, personal or in another pool. | Do not retry; offer only the units the org stock marks `logistician: true`. |
| `HOLDER_UNKNOWN` | An org-stock `book-in` or `move-holder` names a holder who is not on the receiving unit's roster. | Read the roster again and let the member pick. |
| `INSUFFICIENT_STOCK` | An org-stock op takes or marks more than the entry holds, a stock `share` more than the lot's personal amount, or an `unshare` more than the member's own stock in the pool. | Pull and retry with the entry's amount. |
| `STOCK_OFFERED` | A partial org-stock `stolen` or `move-pool`, or a stock `share` or `unshare`, would leave the stock below what it offers on the Basetool's material exchange. | Take less, or ask the holder to lower the offer in the web. |
| `NOT_A_MEMBER` | A stock `share` or a refinery-order `create` names a unit the member is not a direct member of. | Offer only the units of `GET /me/org-units`. |
| `QUANTITY_ABOVE_EXPECTED` | A stock `share` or `unshare` moves more than its `expectedQuantity`. | Move at most the amount the client last saw at the source. |
| `OVER_ALLOCATION` | The earmarks of one kind of an org-stock `book-in` add up to more than its amount, or an earmark op would earmark more of one kind than the entry holds. | Lower the earmarks. |
| `EARMARK_TARGET_INVALID` | An org-stock earmark names a job order that does not need the material or item, an unknown mission or a hidden mission for a new earmark, a mission for an item, a target twice, one the entry is not earmarked to, or, for `allocation-add`, one it already is. | Read the [targets](resources/org-stock.md#the-targets) and the entry again. |
| `ITEM_RETIRED` | The item is no longer in the game, or the Basetool hid it: an org-stock `book-in` of it, or a stock rise of it that the batch's falls of the same item do not cover. | Do not retry; tell the member the item can no longer be booked in. Lowering or moving the stock they hold still works. |
| `MATERIAL_RETIRED` | The material is no longer in the game, or the Basetool hid it, for example a commodity no game source knows: an org-stock `book-in` of it, or a stock rise of it that the batch's falls of the same material do not cover. | Do not retry; tell the member the material can no longer be booked in. Lowering or moving the stock they hold still works. |
| `NOT_A_REFINERY` | A refinery order's location is not a refinery the web offers for refinery orders. | Offer only refinery locations. |
| `MATERIAL_NOT_RAW` | A refinery good's input is not a raw material. | Send the raw material the good refines. |
| `ORDER_UNKNOWN` | A refinery-order op names no order of the member: an unknown `id`, another owner's order, or a `clientKey` your client never set. | Pull the feed; drop the order locally when it is gone. |
| `GOOD_UNKNOWN` | A refinery-order `good-update` or `good-remove` names a `goodId` that is not a good of the order. | Pull the feed and use the good ids it shows. |
| `ALREADY_EXISTS` | A refinery-order `create` carries a `clientKey` an order of the member already has; outside a dry run the result item's `id` names that order. | Adopt the order by that `id`; do not create it again. |
| `GOODS_REQUIRED` | A refinery-order `create` without goods, or a `good-remove` of the order's last good. | Send the order with at least one good; replace a good with `good-update` instead of removing the last one. |
| `PRECEDING_OP_REJECTED` | An earlier op of the change set on the same refinery order, or a refused `create` under the same `clientKey`, was not applied, so this one was not tried. | Fix the first refused op, then resend the rest. |
| `ORDER_STORED` | A refinery-order op changes the status of or deletes an order whose yield was booked into stock, or picks up with store lines an order that is completed or stored. | Do not retry; the status is locked and the order stays. A `cancel` is still possible. |
| `ORDER_CANCELED` | A refinery-order `pickup` names an order that was cancelled. | Do not retry; pull the feed for the order's status. |
| `MATERIAL_NOT_AN_OUTPUT` | A store line of a refinery-order `pickup` books a material that is no output of the order's goods. | Book the goods' output materials as the feed shows them. |
| `RECEIVER_UNKNOWN` | A store line names a receiver who is not among the member's [receivers](resources/store-choices.md#receivers), or a ref that names nobody. | Read the receivers again and let the member pick. |
| `ORG_UNIT_NOT_ALLOWED` | A store line names a unit that is neither a direct membership of its receiver nor a unit the member is a Logistician of, or names none for a receiver with several memberships. | Offer the receiver's `orgUnits` from the receivers read. |
| `JOB_ORDER_INVALID` | A store line's job order is unknown, not open or in progress, outside the member's view, or does not need the line's material. | Read the [job orders](resources/store-choices.md#job-orders) for the material again. |

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
