# Security Policy

[Deutsch](SECURITY.md) · **English**

## Reporting a vulnerability

Please report security issues privately via GitHub's
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new)
rather than as a public issue. You will get an answer within a few days.

## How releases are built

Every published binary is built by a **public GitHub Actions workflow**
([`.github/workflows/release.yml`](.github/workflows/release.yml)),
triggered by a git tag. Local builds are never published. This means each
released file can be traced back to one commit and one workflow run, and
the build log is public.

Released artifacts:

| File | Platform | Built with |
|---|---|---|
| `SC-BP-Watcher-Setup.exe` | Windows | PyInstaller + Inno Setup |
| `SC-BP-Watcher-x86_64.AppImage` | Linux | PyInstaller + AppImage |

## Dependencies

There are none. The program uses the Python standard library only — no
third-party packages, no network libraries beyond `urllib`, and no
proprietary components. This is a deliberate project rule, not a
coincidence.

## What the program sends

Nothing about you. It reads the game's log file locally and fetches
public crafting data from `scmdb.net` and release information from the
GitHub API. The built-in diagnostic report is written to a file for you
to paste yourself — it is never transmitted, and usernames and paths are
replaced before it is shown.

## Antivirus false positives

PyInstaller executables are regularly flagged by machine-learning
detections such as `Trojan:Win32/Wacatac.C!ml`. These are false
positives; the full source is in this repository and the build is
public. If you encounter one, please report it to your vendor — and feel
free to open an issue so other users can find the information.
