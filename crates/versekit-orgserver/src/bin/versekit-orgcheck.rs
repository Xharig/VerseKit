// VerseKit — companion for Star Citizen.
// Copyright (C) 2026 Xharig
//
// SPDX-License-Identifier: GPL-3.0-only
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! `versekit-orgcheck <address>`: whether an org server follows the open standard versekit-org v1, as a list
//! green / red with the reason. Twice it asks a person to confirm a code on the org's page with a test member.

use versekit_orgserver::conformance::{Options, Prompt, run};
use versekit_orgserver::net::Client;

const HELP_DE: &str = "versekit-orgcheck <adresse> [--no-write] [--burst N] [--de|--en]

Prüft, ob ein Orga-Server dem offenen Standard versekit-org v1 folgt: Discovery, Anmeldung per Code,
jede Operation mit ihren Fehlercodes, Rechte-Grenzen, 429 und das Erneuern von Tokens.

Nimm ein Testmitglied: Zweimal erscheint ein Code, den du auf der Seite der Orga bestätigst.
Beim ersten Mal nur das eine Recht, beim zweiten Mal alle. Die schreibenden Prüfungen teilen
kurz einen Testeintrag als eigenen Bestand und eigene Baupläne und nehmen ihn wieder zurück;
--no-write lässt sie aus. --burst N schickt N schnelle Anfragen, um die Bremse zu sehen (0: aus).
Ende mit 0, wenn nichts rot ist.";

const HELP_EN: &str = "versekit-orgcheck <address> [--no-write] [--burst N] [--de|--en]

Checks whether an org server follows the open standard versekit-org v1: discovery, login by code,
every operation with its error codes, the limits of the rights, 429 and the refresh of tokens.

Use a test member: twice a code appears that you confirm on the org's page. The first time only
the one right, the second time all of them. The writing checks share a test entry as own stock and
own blueprints for a moment and take it back; --no-write leaves them out. --burst N sends N quick
requests to see the rate limit (0: off). Exits with 0 when nothing is red.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lang = std::env::var("LANG").unwrap_or_default();
    let mut options = Options { german: lang.to_ascii_lowercase().starts_with("de"), ..Options::default() };
    let mut address = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--de" => options.german = true,
            "--en" => options.german = false,
            "--no-write" => options.write = false,
            "--burst" => match rest.next().and_then(|n| n.parse().ok()) {
                Some(n) => options.burst = n,
                None => {
                    eprintln!("--burst needs a number");
                    std::process::exit(2);
                }
            },
            "-h" | "--help" => {
                println!("{}", if options.german { HELP_DE } else { HELP_EN });
                return;
            }
            other if address.is_none() && !other.starts_with('-') => address = Some(other.to_string()),
            other => {
                eprintln!("{other}?\n\n{}", if options.german { HELP_DE } else { HELP_EN });
                std::process::exit(2);
            }
        }
    }
    let Some(address) = address else {
        eprintln!("{}", if options.german { HELP_DE } else { HELP_EN });
        std::process::exit(2);
    };
    let german = options.german;
    let mut confirm = |prompt: &Prompt| {
        if german {
            println!(
                "\nBitte jetzt bestätigen: {}\n  Code: {}\n  Rechte: {}\n  (Ich warte, bis die Bestätigung da ist.)\n",
                prompt.verification_uri,
                prompt.user_code,
                prompt.scopes.join(", ")
            );
        } else {
            println!(
                "\nPlease confirm now: {}\n  Code: {}\n  Rights: {}\n  (Waiting for the confirmation.)\n",
                prompt.verification_uri,
                prompt.user_code,
                prompt.scopes.join(", ")
            );
        }
    };
    let wait = |seconds: u64| std::thread::sleep(std::time::Duration::from_secs(seconds));
    let report = run(&address, &Client::new(), &mut confirm, &wait, &options);
    print!("\n{}", report.render(german));
    std::process::exit(if report.passed() { 0 } else { 1 });
}
