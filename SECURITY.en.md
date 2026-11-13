# Security Policy

[Deutsch](SECURITY.md) · **English**

## Reporting a vulnerability

Please report security issues privately via GitHub's
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new)
rather than as a public issue. You will get an answer within a few days.

## Credentials for the KRT Profit Basetool

If you connect VerseKit to the KRT Profit Basetool, it holds an access token
and a key of its own. Both are kept **only** in your system's key store: on
Windows encrypted with DPAPI and as a non-exportable key (in the TPM where
there is one), on Linux in the Secret Service. Only where Linux has no Secret
Service, or `secret-tool` is missing or does not answer, are they kept in a
file that you alone can read, and VerseKit shows you that it does. If the
Secret Service refuses, VerseKit does not quietly fall back to the file. They
never appear in your VerseKit data, a backup, the error log or a problem
report; anything shaped like a token is redacted there.

A reported flaw in how these credentials are handled is fixed in a published
release **within seven days**.

## How releases are built

Every published binary is built by a **public GitHub Actions workflow**
([`.github/workflows/release.yml`](.github/workflows/release.yml)),
triggered by a git tag. Local builds are never published. This means each
released file can be traced back to one commit and one workflow run, and
the build log is public. Every release carries a `SHA256SUMS.txt`; VerseKit
only accepts it from GitHub in this repository and installs no update whose
checksum does not match: the file is then deleted and nothing is installed.
Right before installing, VerseKit checks the sum once more.

Released artifacts:

| File | Platform | Built with |
|---|---|---|
| `VerseKit-Setup.exe` | Windows | Rust (`cargo build --release`) + Inno Setup |
| `VerseKit-x86_64.AppImage` | Linux | Rust (`cargo build --release`) + AppImage |

### Verifying the build provenance yourself

For both files and for `SHA256SUMS.txt`, the build run creates a **build
provenance** (GitHub Artifact Attestation, signed through Sigstore and entered
into the public transparency log). It proves that exactly this file was built
by `.github/workflows/release.yml` in this repository for the tag of the
release. This holds for releases made by pushing a tag, the usual way. When
another workflow starts the build (`geplant.yml`), the provenance names its
branch instead of the tag, and VerseKit does not accept it. GitHub keeps it
apart from the release files: whoever only swaps the
attachments of a release cannot forge it along with them.

With the [GitHub CLI](https://cli.github.com) you check a downloaded file
like this:

```bash
gh attestation verify VerseKit-Setup.exe --repo Xharig/VerseKit
```

VerseKit asks GitHub for the provenance itself after every update download
(for that the file's checksum goes to api.github.com, and the provenance
bundle comes from GitHub's storage at Microsoft Azure) and checks signature,
certificate, the transparency log entry, repository, workflow and tag. The
result is written to the update log `update-helfer.txt`; the problem report
shows its last eight lines under “Last update attempt”. If a provenance exists
but does not hold up, that also goes into the error log.

For now the provenance does not stop an update, neither a missing nor a wrong
one nor one GitHub cannot deliver at the moment, because older releases have
none. The checksum, however, always has to match.

## Dependencies

The program is written in Rust. Every dependency is pinned with its version and
checksum in `Cargo.lock`; the build runs with `--locked`, and `deny.toml` states
which licences and sources are allowed.

Three third-party parts are shipped, each with its licence:

| What | Licence | Where |
|---|---|---|
| Icons from [Lucide](https://lucide.dev) | ISC | `tools/symbol-vorlagen/` |
| Flags from [flag-icons](https://flagicons.lipis.dev) | MIT | `tools/flaggen-vorlagen/` |
| DPoP reference of the [KRT Profit Basetool](https://krt-profit.github.io/basetool/) | MIT | `crates/versekit-core/licenses/` |

## What the program sends

See the [privacy statement](PRIVACY.en.md).

## Antivirus false positives

Self-built, unsigned executables are occasionally flagged by machine-learning
detections such as `Trojan:Win32/Wacatac.C!ml`. These are false
positives; the full source is in this repository and the build is
public. If you encounter one, please report it to your vendor — and feel
free to open an issue so other users can find the information.
