# Security Policy

[Deutsch](SECURITY.md) · **English**

## Reporting a vulnerability

Please report security issues privately via GitHub's
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new)
rather than as a public issue. You will get an answer within a few days.

## Credentials for the KRT Profit Basetool

If you connect Verse-Kit to the KRT Profit Basetool, it holds an access token
and a key of its own. Both are kept **only** in your system's key store: on
Windows encrypted with DPAPI and as a non-exportable key (in the TPM where
there is one), on Linux in the Secret Service. Only where Linux has no Secret
Service are they kept in a file that you alone can read — and Verse-Kit shows
you that it does. They never appear in your Verse-Kit data, a backup, the error
log or a problem report; anything shaped like a token is redacted there.

A reported flaw in how these credentials are handled is fixed in a published
release **within seven days**.

## How releases are built

Every published binary is built by a **public GitHub Actions workflow**
([`.github/workflows/release.yml`](.github/workflows/release.yml)),
triggered by a git tag. Local builds are never published. This means each
released file can be traced back to one commit and one workflow run, and
the build log is public. Every release carries a `SHA256SUMS.txt`; Verse-Kit
installs no update whose checksum does not match.

Released artifacts:

| File | Platform | Built with |
|---|---|---|
| `VerseKit-Setup.exe` | Windows | PyInstaller + Inno Setup |
| `VerseKit-x86_64.AppImage` | Linux | PyInstaller + AppImage |

## Dependencies

The program uses the Python standard library only — no third-party packages,
no network libraries beyond `urllib`, and no proprietary components. This is a
deliberate project rule, not a coincidence.

Only three third-party parts are shipped, each with its licence:

| What | Licence | Where |
|---|---|---|
| Icons from [Lucide](https://lucide.dev) | ISC | `tools/symbol-vorlagen/` |
| Flags from [flag-icons](https://flagicons.lipis.dev) | MIT | `tools/flaggen-vorlagen/` |
| DPoP reference of the [KRT Profit Basetool](https://krt-profit.github.io/basetool/) | MIT | `scbp/dpop_reference/` |

## What the program sends

See the [privacy statement](PRIVACY.en.md).

## Antivirus false positives

PyInstaller executables are regularly flagged by machine-learning
detections such as `Trojan:Win32/Wacatac.C!ml`. These are false
positives; the full source is in this repository and the build is
public. If you encounter one, please report it to your vendor — and feel
free to open an issue so other users can find the information.
