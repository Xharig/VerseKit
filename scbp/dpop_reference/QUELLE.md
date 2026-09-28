# DPoP-Referenz (übernommen)

Signiert die Nachweise (DPoP, RFC 9449, ES256), mit denen VerseKit sich beim
KRT Profit Basetool ausweist, und hält den Schlüssel der Installation dort, wo
das System Schlüssel hält: unter Windows als nicht exportierbarer CNG-Schlüssel,
unter Linux über OpenSSL 3.

| | |
|---|---|
| Quelle | [krt-profit/basetool](https://github.com/krt-profit/basetool), `docs/exchange/dpop-reference/` |
| Stand | Commit `f799e1e4ffe3f4e0ad8f055cca51f2534c1eac8a` (28.09.2026) |
| Lizenz | MIT — siehe `LICENSE` in diesem Ordner |
| Urheber | greluc (KRT) |

**Nicht von Hand ändern.** Die Dateien sind unverändert übernommen; ändert sich
die Referenz drüben, wird sie neu übernommen und der Commit oben nachgetragen.
Was VerseKit darüber hinaus braucht (Schlüssel im Secret Service statt in einer
Datei, Token-Ablage, Anmeldung), steht in `scbp/basetool.py` und
`scbp/secret_store.py`.
