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

//! A made-up installation of Star Citizen with a data folder of Verse-Kit, in a throwaway folder: the pages
//! that read the game folder (`Game.log`, `user.cfg`, `attributes.xml`, `actionmaps.xml`) and the data folder
//! (settings, price lists) show something on it. Everything in it is invented except the price lists of the
//! market, which are public data and copied from a folder given with `--markt`; nothing of the computer is
//! read or written outside the throwaway folder, and the Basetool cannot go out.

use std::path::{Path, PathBuf};

use versekit_core::legacy::{ENV_HOME, ENV_NO_NET};
use versekit_core::paths::{ENV_INSTALL_DIR, Host};

/// The sticks and the pedals of the made-up installation: slot, name, id. They are the ones plugged into the
/// computer the pictures are made on: the page "Steuerung" lists what the system reports next to what the game
/// knows, and the two must be the same devices.
const DEVICES: [(u32, &str, &str); 3] = [
    (0, "L-VPC Stick WarBRD-D", "03F33344-0000-0000-0000-504944564944"),
    (1, "R-VPC Stick WarBRD-D", "43F43344-0000-0000-0000-504944564944"),
    (2, "VPC Rudder Pedals", "01F83344-0000-0000-0000-504944564944"),
];

/// The handle of the made-up player.
const PLAYER: &str = "Beispielspieler";

/// The price lists of the market and the slots and parts of the ships (read from the game data and from erkul)
/// that are copied from the folder given: public data, no data of the player.
const MARKET_FILES: [&str; 11] = [
    "laeden-katalog.json",
    "laeden.json",
    "schiffe.json",
    "verkauf.json",
    "preise.json",
    "bergung.json",
    "spiel-schiffe.json",
    "spiel-teile.json",
    "erkul-schiffe.json",
    "erkul-teile.json",
    "werte-spieldaten.json",
];

/// The texts of the game (`global.ini`): a contract title with the blueprint mark of this tool and the names of the
/// ships of the made-up fleet.
const GLOBAL_INI: &str = "mission_title_retake=Retake the Outpost <EM4>[BP 2/3]</EM4>\n\
vehicle_NameANVL_Hornet_F7C_M_Mk2=F7C-M Super Hornet Mk II\n\
vehicle_NameRSI_Meteor=Meteor\n\
vehicle_NameDRAK_Cutlass_Black=Cutlass Black\n\
vehicle_NameMISC_Prospector=Prospector\n\
vehicle_NameAEGS_Avenger_Titan=Avenger Titan\n";

/// The own names of the made-up fleet (`asop.json`): the Super Hornet named and starred, so it stands first.
const SHIP_NAMES: &str =
    r#"{"format": 1, "namen": {"vehicle_NameANVL_Hornet_F7C_M_Mk2": {"name": "Nachtfalke", "stern": true}}}
"#;

/// The fleet of the made-up player (`hangar.json`): ships with slots in the game data, nothing equipped yet.
const HANGAR: &str = r#"{"format": 1, "schiffe": [
 {"name": "F7C-M Super Hornet Mk II", "hersteller": "Anvil Aerospace", "hkurz": "ANVL", "kurz": "ANVL_Hornet_F7CM_Mk2", "herkunft": "pledge", "lti": true, "einsatzbereit": true, "belegung": {}},
 {"name": "Meteor", "hersteller": "Roberts Space Industries", "hkurz": "RSI", "kurz": "RSI_Meteor", "herkunft": "pledge", "lti": true, "belegung": {}},
 {"name": "Cutlass Black", "hersteller": "Drake Interplanetary", "hkurz": "DRAK", "kurz": "DRAK_Cutlass_Black", "herkunft": "pledge", "versicherung": 120, "einsatzbereit": true, "belegung": {}},
 {"name": "Prospector", "hersteller": "Musashi Industrial & Starflight Concern", "hkurz": "MISC", "kurz": "MISC_Prospector", "herkunft": "pledge", "versicherung": 6, "belegung": {}},
 {"name": "Avenger Titan", "hersteller": "Aegis Dynamics", "hkurz": "AEGS", "kurz": "AEGS_Avenger_Titan", "herkunft": "ingame", "belegung": {}}]}
"#;

/// What the log of the game says at its start: the account, the computer, the graphics card, the class of
/// the game and the connected devices.
fn game_log() -> String {
    let mut text = String::new();
    text.push_str(&format!("<Legacy login response> [CIG-net] User Login Success - Handle[{PLAYER}] - Time\n"));
    for line in [
        "Host CPU: AMD Ryzen 7 5800X3D 8-Core Processor",
        "Logical CPU Count: 16",
        "32678MB physical memory installed",
        "[VK] Chosen Vulkan GPU Device (NVIDIA GeForce RTX 4070) Driver Version (581.57.0) Vulkan API (1.4.312)",
        "GPU: DedicatedVidMemMB = 12282",
        "Current display mode is 2560x1440",
        "GPU: DLSS Support = Available",
        "CPU benchmark: 12.5 ms (int+mem), 9.25 ms (fp+mem)",
        "GPU benchmark: 3.5 ms",
        "Performance Index: 171.40 (CPU), 448.90 (GPU)",
        "Final rating: Machine class 4",
    ] {
        text.push_str(&format!("<2026-11-01T19:02:11.000Z> {line}\n"));
    }
    for (slot, name, id) in DEVICES {
        text.push_str(&format!("Connected joystick{slot}:  {name}  {{{id}}}\n"));
    }
    text
}

/// The file `actionmaps.xml` of the game: the devices with their dead zone and saturation, and the bindings of
/// the flight axes.
fn actionmaps() -> String {
    let mut text = String::from(
        "<ActionMaps>\n <ActionProfiles version=\"1\" optionsVersion=\"2\" rebindVersion=\"2\" profileName=\"default\">\n",
    );
    for (slot, name, id) in DEVICES {
        let (dead, saturation) = if slot == 2 { ("0.1", "0.9") } else { ("0.15", "0.85") };
        text.push_str(&format!("  <deviceoptions name=\"{name}  {{{id}}}\">\n"));
        for axis in ["x", "y", "rotz"] {
            text.push_str(&format!("   <option input=\"{axis}\" deadzone=\"{dead}\"/>\n"));
            text.push_str(&format!("   <option input=\"{axis}\" saturation=\"{saturation}\"/>\n"));
        }
        text.push_str("  </deviceoptions>\n");
    }
    for (slot, name, id) in DEVICES {
        text.push_str(&format!(
            "  <options type=\"joystick\" instance=\"{}\" Product=\"{name}  {{{id}}}\">\n",
            slot + 1
        ));
        text.push_str("   <flight_move_pitch exponent=\"1.5\"/>\n   <flight_move_roll exponent=\"1.2\"/>\n");
        text.push_str("  </options>\n");
    }
    text.push_str(
        "  <actionmap name=\"spaceship_movement\">\n\
   <action name=\"v_pitch\"><rebind input=\"js1_y\"/></action>\n\
   <action name=\"v_roll\"><rebind input=\"js1_x\"/></action>\n\
   <action name=\"v_yaw\"><rebind input=\"js3_x\"/></action>\n\
   <action name=\"v_strafe_vertical\"><rebind input=\"js2_y\"/></action>\n\
   <action name=\"v_strafe_lateral\"><rebind input=\"js2_x\"/></action>\n\
   <action name=\"v_eject\"><rebind input=\"js1_button10\"/></action>\n\
  </actionmap>\n </ActionProfiles>\n</ActionMaps>\n",
    );
    text
}

/// The file `attributes.xml` of the game: the graphics settings the page of the graphics reads and the field of
/// view the page "Blickwinkel" reads.
const ATTRIBUTES: &str = "<Attributes Version=\"35\">\r\n <Attr name=\"FOV\" value=\"58.0\"/>\r\n \
<Attr name=\"Width\" value=\"2560\"/>\r\n <Attr name=\"Height\" value=\"1440\"/>\r\n \
<Attr name=\"WindowMode\" value=\"2\"/>\r\n <Attr name=\"VSync\" value=\"0\"/>\r\n \
<Attr name=\"SysSpec\" value=\"3\"/>\r\n <Attr name=\"Upscaling\" value=\"0\"/>\r\n \
<Attr name=\"MotionBlur\" value=\"0\"/>\r\n <Attr name=\"FilmGrain\" value=\"0\"/>\r\n \
<Attr name=\"Sharpening\" value=\"0.3\"/>\r\n <Attr name=\"ChromaticAberration\" value=\"0\"/>\r\n \
<Attr name=\"SysSpec_ShadowMaps\" value=\"3\"/>\r\n <Attr name=\"SysSpec_TextureQuality\" value=\"4\"/>\r\n \
<Attr name=\"SysSpec_ObjectViewDistance\" value=\"3\"/>\r\n</Attributes>\r\n";

/// The file `user.cfg` of the game.
const USER_CFG: &str = "r_DisplayInfo = 0\nsys_MaxFPS = 60\nr_VSync = 0\ng_language = german_(germany)\n\
g_languageAudio = english\nsys_MaxIdleFPS = 60\n";

/// What the settings file of the data folder gets in addition to the base: the field of view measured by hand
/// and the start programs.
fn extra_settings(tools: &Path) -> String {
    let program = |name: &str, file: &str, when: &str, wait: u32, end: bool| {
        format!(
            "{{\"an\": true, \"name\": \"{name}\", \"datei\": {file:?}, \"argumente\": \"\", \"wann\": \"{when}\", \"warten\": {wait}, \"beenden\": {end}}}"
        )
    };
    let voice = tools.join("VoiceAttack").join("VoiceAttack.exe").to_string_lossy().into_owned();
    let tracker = tools.join("TrackIR").join("TrackIR5.exe").to_string_lossy().into_owned();
    format!(
        ",\n  \"fov_mm_je_pixel\": 0.2331,\n  \"fov_pixelbreite\": 2560,\n  \"fov_abstand_mm\": 700,\n  \
         \"startprogramme\": [{}, {}]",
        program("VoiceAttack", &voice, "spiel", 5, true),
        program("TrackIR", &tracker, "launcher", 0, true)
    )
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("Ordner");
    }
    std::fs::write(path, text).expect("Datei");
}

/// `text` with the number after every `"geholt":` (the time a list was fetched, in Unix seconds, with or without a
/// blank after the colon) set to `time`.
fn with_fetch_time(text: &str, time: f64) -> String {
    const KEY: &str = "\"geholt\":";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(KEY) {
        let (head, tail) = rest.split_at(at + KEY.len());
        out.push_str(head);
        let blanks = tail.len() - tail.trim_start_matches(' ').len();
        let (blank, tail) = tail.split_at(blanks);
        out.push_str(blank);
        let number = tail.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E' | '+')));
        let (old, after) = tail.split_at(number.unwrap_or(tail.len()));
        // A value that is no number (null) stays as it is.
        if old.is_empty() {
            rest = tail;
            continue;
        }
        out.push_str(&format!("{time:.3}"));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The throwaway folder: `C:\Games` where that is free (the pictures show paths, and a path in the home folder
/// would show the name of the user), else a folder of its own in the root of the system drive, else one in the
/// temporary folder. The folder is new; an existing one is never used.
fn make_folder() -> PathBuf {
    let root = std::env::var("SystemDrive").map(|d| PathBuf::from(format!("{d}\\"))).ok();
    if let Some(root) = &root {
        let games = root.join("Games");
        if !games.exists() && std::fs::create_dir(&games).is_ok() {
            return games;
        }
    }
    let name = format!("vk-bilder-{}", std::process::id());
    let dir = root.unwrap_or_else(std::env::temp_dir).join(&name);
    if std::fs::create_dir_all(&dir).is_ok() {
        return dir;
    }
    let dir = std::env::temp_dir().join(&name);
    std::fs::create_dir_all(&dir).expect("Ordner");
    dir
}

/// Builds the installation. Gives the folder (to remove at the end) and the host that sees only it. `settings`
/// is the text of the settings file of the data folder, `market` a folder with the price lists of the market
/// (copied, read only).
pub fn build(settings: &str, market: Option<&Path>) -> (PathBuf, Host) {
    let dir = make_folder();
    let game = dir.join("StarCitizen").join("LIVE");
    let data = dir.join("VerseKit");
    let tools = dir.join("Tools");
    std::fs::create_dir_all(&data).expect("Datenordner");
    write(&game.join("Game.log"), &game_log());
    write(&game.join("user.cfg"), USER_CFG);
    write(
        &game.join("build_manifest.id"),
        "{\"Data\":{\"Branch\":\"sc-alpha-4.3.1\",\"RequestedP4ChangeNum\":\"9876543\"}}\n",
    );
    write(&game.join("USER/Client/0/Profiles/default/actionmaps.xml"), &actionmaps());
    write(&game.join("USER/Client/0/Profiles/default/attributes.xml"), ATTRIBUTES);
    // The texts of the game with one contract title that carries the blueprint mark of this tool, and the names
    // of the ships of the made-up fleet.
    for lang in ["english", "german_(germany)"] {
        write(&game.join("data/Localization").join(lang).join("global.ini"), GLOBAL_INI);
    }
    // Two kept logs of the same player: the accounts the page of the detection counts.
    for name in ["Game Build(9876543) 30 Oct 26 (20 11 02).log", "Game Build(9876543) 31 Oct 26 (19 40 17).log"] {
        write(
            &game.join("logbackups").join(name),
            &format!("<Legacy login response> [CIG-net] User Login Success - Handle[{PLAYER}] - Time\n"),
        );
    }
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    std::fs::File::options().write(true).open(game.join("Game.log")).expect("Log").set_modified(old).expect("Zeit");
    for file in ["VoiceAttack/VoiceAttack.exe", "TrackIR/TrackIR5.exe"] {
        write(&tools.join(file), "");
    }
    // The source of the game texts: the German translation of the Luftwerft, chosen, with a status that says when it
    // was fetched and looked at (today, so that the page says "heute").
    let base = settings.replace("\"inj_quelle\": \"deutsch\"", "\"inj_quelle\": \"luftwerft_de\"");
    let base = base.trim_end().trim_end_matches('}').trim_end();
    write(&data.join("einstellungen.json"), &format!("{base}{}\n}}\n", extra_settings(&tools)));
    write(&data.join("hangar.json"), HANGAR);
    write(&data.join("asop.json"), SHIP_NAMES);
    let today = versekit_core::translation::today();
    write(
        &data.join(versekit_core::translation::NOTE_FILE),
        &format!(
            "{{\n \"luftwerft_de\": {{\n  \"kennung\": \"etag:ac5845-65e587186a901\",\n  \"stand\": \"{today} 13:33\",\n  \
             \"sprache\": \"german_(germany)\",\n  \"datum\": \"2026-10-22\",\n  \"geprueft\": \"{today} 13:41\",\n  \
             \"pruefung\": \"aktuell\"\n }}\n}}\n"
        ),
    );
    if let Some(market) = market {
        // The lists count as fetched two hours ago, as in a program that keeps them up to date.
        let fetched = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() - 7200.0);
        for file in MARKET_FILES {
            match std::fs::read_to_string(market.join(file)) {
                Ok(text) => write(&data.join(file), &with_fetch_time(&text, fetched)),
                Err(e) => println!("Marktdatei {file} nicht kopiert: {e}"),
            }
        }
    }
    let (home, data, game) =
        (dir.to_string_lossy().into_owned(), data.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let host = Host::new(
        versekit_core::pypath::Flavor::native(),
        &[
            (ENV_HOME, &data),
            ("HOME", &home),
            ("USERPROFILE", &home),
            (ENV_NO_NET, "1"),
            (ENV_INSTALL_DIR, &game),
            (versekit_core::game_start::ENV_NO_PROCESS, "1"),
        ],
        &home,
    );
    (dir, host)
}
