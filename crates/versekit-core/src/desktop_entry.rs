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

//! An entry in the application menu of a Linux desktop (`desktop_entry.py` of 4.12).
//!
//! A `.desktop` file of the Freedesktop standard in `$XDG_DATA_HOME/applications` (by default
//! `~/.local/share/applications`): the place of the entries of one user, no system folder, no rights needed,
//! and taking it away leaves nothing but that one file. The program icon is copied next to it into the icon
//! folder of the user: an AppImage carries its icon, but the menu cannot look inside.
//!
//! Every function that touches files takes the data folder of the user, so that a test runs on a throwaway
//! folder; the ones that read the machine ([`create`], [`remove`], [`present`]) take the [`Host`].

use std::path::{Path, PathBuf};

use crate::language::{self, Lang};
use crate::legacy;
use crate::paths::{Host, WM_CLASS};

/// The file of the entry.
pub const FILE_NAME: &str = "versekit.desktop";

/// The file name of the icon copied next to the entry.
pub const ICON_FILE: &str = "versekit.png";

/// The name of the icon in the theme: used when no icon file could be found to copy.
const ICON_NAME: &str = "versekit";

/// Does the entry make sense on this system? A system with menus of `.desktop` files: not Windows (the installer
/// sets everything up) and not macOS.
pub fn available() -> bool {
    cfg!(not(any(windows, target_os = "macos")))
}

/// The data folder of the user: `$XDG_DATA_HOME`, by default `~/.local/share`.
pub fn data_home(host: &Host) -> PathBuf {
    match host.var_set("XDG_DATA_HOME") {
        Some(base) => PathBuf::from(base),
        None => PathBuf::from(host.home()).join(".local").join("share"),
    }
}

/// The folder of the menu entries of the user.
pub fn menu_dir(data_home: &Path) -> PathBuf {
    data_home.join("applications")
}

/// The file of the entry.
pub fn target_file(data_home: &Path) -> PathBuf {
    menu_dir(data_home).join(FILE_NAME)
}

/// The folder the icon is copied into.
pub fn icon_dir(data_home: &Path) -> PathBuf {
    data_home.join("icons").join("hicolor").join("256x256").join("apps")
}

/// The program the entry starts: the AppImage (the variable `APPIMAGE`, only while it is a file: the unpacked
/// program lies in a folder that has another name at the next start), else the running program.
pub fn program(host: &Host, current_exe: &Path) -> PathBuf {
    match host.var_set("APPIMAGE").map(PathBuf::from) {
        Some(image) if image.is_file() => image,
        _ => current_exe.to_path_buf(),
    }
}

/// The switches a start of this program needs to land in the run that is going: `--daten <copy>` on a copy of
/// the data ([`Host::copy`]), nothing on the user's own data. Without them a start from the menu, the login or a
/// shortcut of the desktop runs on the user's own data, beside the copy.
pub fn run_arguments(host: &Host) -> Vec<String> {
    host.copy.iter().flat_map(|copy| ["--daten".to_string(), copy.clone()]).collect()
}

/// The command of the entry: the path and each of `arguments` as one quoted argument ([`exec_argument`]).
pub fn command(program: &Path, arguments: &[String]) -> String {
    let mut line = exec_argument(&program.to_string_lossy());
    for argument in arguments {
        line.push(' ');
        line.push_str(&exec_argument(argument));
    }
    line
}

/// One argument of an `Exec=` line, after the Desktop Entry Specification ("The Exec key"): in double quotes (a
/// path with a space breaks the start without them), where `"`, a backtick, `$` and `\` get a backslash; a `%`
/// doubled (the menu reads `%f` and the like as placeholders); and then the escapes of every string value of the
/// file, which double each backslash and write a line break as `\n`. So a `$` in a path stands as `\\$`, a
/// backslash as four. The same for the menu entry and for the start with the login.
pub fn exec_argument(text: &str) -> String {
    let quoted = quote(text, true);
    let mut value = String::with_capacity(quoted.len());
    for c in quoted.chars() {
        match c {
            '\\' => value.push_str("\\\\"),
            '\n' => value.push_str("\\n"),
            '\t' => value.push_str("\\t"),
            '\r' => value.push_str("\\r"),
            _ => value.push(c),
        }
    }
    value
}

/// `text` in double quotes, a backslash before `"`, a backtick, `$` and `\`; with `percent` a `%` doubled.
fn quote(text: &str, percent: bool) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' if percent => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// The program and each of `arguments` as one argument for the field of a shortcut of the desktop: in double
/// quotes with a backslash before `"`, a backtick, `$` and `\`, which a shell, the shortcuts of KDE and those of
/// GNOME all read as the one path.
pub fn typed_command(program: &Path, arguments: &[String]) -> String {
    let mut line = quote(&program.to_string_lossy(), false);
    for argument in arguments {
        line.push(' ');
        line.push_str(&quote(argument, false));
    }
    line
}

/// Is the session a Wayland one? There no program can take a key for the whole desktop by itself.
pub fn wayland(host: &Host) -> bool {
    host.var_set("XDG_SESSION_TYPE").is_some_and(|kind| kind.eq_ignore_ascii_case("wayland"))
        || host.var_set("WAYLAND_DISPLAY").is_some()
}

/// The command for a shortcut of the desktop on this machine, only where it is needed: a Wayland session on a
/// system with menus of `.desktop` files.
pub fn shortcut_command(host: &Host) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    (available() && wayland(host)).then(|| typed_command(&program(host, &exe), &run_arguments(host)))
}

/// The escapes of a string value undone: `\\`, `\s`, `\n`, `\t`, `\r`; any other backslash stays.
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The program an `Exec=` line starts: the first argument, in quotes (with its backslashes undone) or up to the
/// first space; the reverse of [`exec_argument`].
pub fn exec_program(command: &str) -> String {
    let line = unescape_value(command.trim());
    let first = match line.strip_prefix('"') {
        Some(rest) => {
            let mut argument = String::new();
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => argument.extend(chars.next()),
                    _ => argument.push(c),
                }
            }
            argument
        }
        None => line.split(' ').next().unwrap_or_default().to_string(),
    };
    first.replace("%%", "%")
}

/// Is the entry there, and does it still point to a program that is there? A file without an `Exec=` line
/// counts as there; one that cannot be read does not.
pub fn exists(data_home: &Path) -> bool {
    let file = target_file(data_home);
    if !file.is_file() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&file) else { return false };
    match text.lines().find_map(|line| line.strip_prefix("Exec=")) {
        Some(command) => Path::new(&exec_program(command)).exists(),
        None => true,
    }
}

/// [`exists`] for this machine.
pub fn present(host: &Host) -> bool {
    exists(&data_home(host))
}

/// The text of a `.desktop` file, without file system. The `StartupWMClass` is the window class the desktop
/// compares with ([`WM_CLASS`]), not the name shown: with the name the entry would do nothing, no window is
/// called so.
pub fn content(command: &str, icon: &str, name: &str, comment_de: &str, comment_en: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment={comment_de}\nComment[en]={comment_en}\n\
         Exec={command}\nIcon={icon}\nTerminal=false\nCategories=Game;\nStartupWMClass={WM_CLASS}\n\
         Keywords=Star Citizen;Blueprint;Bauplan;\n"
    )
}

/// [`content`] with the name and the subtitle of the program from the language files.
fn content_named(command: &str, icon: &str) -> String {
    content(
        command,
        icon,
        &language::t_in(Lang::De, "hf_titel"),
        &language::t_in(Lang::De, "vk_untertitel"),
        &language::t_in(Lang::En, "vk_untertitel"),
    )
}

/// Where the icon of the program is shipped: inside the AppImage (`$APPDIR`), or next to the program.
fn icon_source(host: &Host, current_exe: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(appdir) = host.var_set("APPDIR") {
        candidates.push(Path::new(appdir).join(ICON_FILE));
    }
    if let Some(folder) = current_exe.parent() {
        candidates.push(folder.join("icon.png"));
        candidates.push(folder.join("assets").join("icon.png"));
    }
    candidates.into_iter().find(|c| c.is_file())
}

/// Copies the icon next to the entry; gives the path to put into the entry, or the name of the icon in the theme
/// when there is none to copy.
fn write_icon(data_home: &Path, source: Option<&Path>) -> String {
    let Some(source) = source else { return ICON_NAME.to_string() };
    let folder = icon_dir(data_home);
    let target = folder.join(ICON_FILE);
    match std::fs::create_dir_all(&folder).and_then(|()| std::fs::copy(source, &target)) {
        Ok(_) => target.to_string_lossy().into_owned(),
        Err(_) => ICON_NAME.to_string(),
    }
}

/// Writes the entry for `program` with `arguments` ([`run_arguments`]) into the data folder; gives the file, or
/// why it did not work.
pub fn create_in(data_home: &Path, program: &Path, arguments: &[String], icon: Option<&Path>) -> Result<PathBuf, String> {
    if !program.exists() {
        return Err(program.to_string_lossy().into_owned());
    }
    let icon = write_icon(data_home, icon);
    let text = content_named(&command(program, arguments), &icon);
    let folder = menu_dir(data_home);
    let file = target_file(data_home);
    let temp = file.with_extension("desktop.teil");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&temp, &file) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.to_string());
    }
    // The desktops want the file executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(file)
}

/// Writes the entry for the running program on this machine and asks the menu to look again: where that tool is
/// missing the entry shows up at the next login, no reason for a fault. On a copy of the data the entry starts the
/// program on that copy ([`run_arguments`]).
pub fn create(host: &Host) -> Result<PathBuf, String> {
    if !available() {
        return Err("not available".to_string());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = data_home(host);
    let file = create_in(&home, &program(host, &exe), &run_arguments(host), icon_source(host, &exe).as_deref())?;
    if let Ok(mut child) = std::process::Command::new("update-desktop-database")
        .arg(menu_dir(&home))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    Ok(file)
}

/// The value of the first line `key=` of a `.desktop` text.
fn value_of<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
}

/// Brings an entry that is there to the name, the subtitles and the window class of this version; gives whether
/// something was written.
///
/// ⛔ Never makes an entry: whoever deleted it keeps it deleted. ⛔ Leaves `Exec`, `Icon` and the file name alone:
/// they point to the program and the icon. Writes only when a line differs (no new time stamp for nothing).
/// Only whether the file is there counts, not where `Exec` points: an AppImage just moved keeps its old words
/// otherwise.
pub fn refresh_label(data_home: &Path) -> bool {
    let wanted = [
        ("Name=", language::t_in(Lang::De, "hf_titel")),
        ("Comment=", language::t_in(Lang::De, "vk_untertitel")),
        ("Comment[en]=", language::t_in(Lang::En, "vk_untertitel")),
        ("StartupWMClass=", WM_CLASS.to_string()),
    ];
    refresh_lines(&target_file(data_home), &wanted)
}

/// [`refresh_label`] with the lines given: each line that starts with one of the keys becomes key and value.
fn refresh_lines(file: &Path, wanted: &[(&str, String)]) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else { return false };
    let mut changed = false;
    let lines: Vec<String> = text
        .split_inclusive('\n')
        .map(|line| {
            let end = if line.ends_with('\n') { "\n" } else { "" };
            match wanted.iter().find(|(key, _)| line.starts_with(key)) {
                Some((key, value)) if line.trim_end_matches(['\r', '\n']) != format!("{key}{value}") => {
                    changed = true;
                    format!("{key}{value}{end}")
                }
                _ => line.to_string(),
            }
        })
        .collect();
    // Written in place: the file keeps the rights it has.
    changed && std::fs::write(file, lines.concat()).is_ok()
}

/// Brings the icon file of an entry that is there to the bytes of `source`; gives whether something was written.
///
/// ⛔ The `Icon=` line is read, never written. Only the own icon file is touched: an absolute path that ends in
/// [`ICON_FILE`] and is a file. Whoever put their own icon there does not point to it any more and keeps theirs.
/// The same bytes: nothing is written.
pub fn refresh_icon(data_home: &Path, source: Option<&Path>) -> bool {
    let Some(source) = source else { return false };
    let Ok(text) = std::fs::read_to_string(target_file(data_home)) else { return false };
    let Some(icon) = value_of(&text, "Icon").map(|v| PathBuf::from(v.trim())) else { return false };
    if !icon.is_absolute() || icon.file_name() != Some(ICON_FILE.as_ref()) || !icon.is_file() {
        return false;
    }
    let (Ok(new), Ok(old)) = (std::fs::read(source), std::fs::read(&icon)) else { return false };
    new != old && std::fs::write(&icon, new).is_ok()
}

/// Takes over the entry of 4.12 ([`legacy::MENU_FILE`]): whoever had one gets the entry of this program (unless
/// one is there already), and the old one goes with its icon, else the menu would show the program twice. When
/// the new entry cannot be written the old one stays. Gives whether something was taken over.
pub fn take_over(data_home: &Path, program: &Path, icon: Option<&Path>) -> bool {
    let old = menu_dir(data_home).join(legacy::MENU_FILE);
    let Ok(text) = std::fs::read_to_string(&old) else { return false };
    if !target_file(data_home).is_file() && create_in(data_home, program, &[], icon).is_err() {
        return false;
    }
    if std::fs::remove_file(&old).is_err() {
        return false;
    }
    let old_icon = icon_dir(data_home).join(legacy::MENU_ICON);
    if value_of(&text, "Icon").map(str::trim) == Some(old_icon.to_string_lossy().as_ref()) {
        let _ = std::fs::remove_file(old_icon);
    }
    true
}

/// What the start of this version does to the entry: takes over the one of 4.12, then brings the words and the
/// icon to this version. Nothing is made where there was none. Gives whether something was written.
pub fn follow_in(data_home: &Path, program: &Path, icon: Option<&Path>) -> bool {
    let taken = take_over(data_home, program, icon);
    let label = refresh_label(data_home);
    let picture = refresh_icon(data_home, icon);
    taken || label || picture
}

/// [`follow_in`] for the running program on this machine; nothing where there are no such menus.
pub fn follow(host: &Host) -> bool {
    if !available() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    follow_in(&data_home(host), &program(host, &exe), icon_source(host, &exe).as_deref())
}

/// Takes the entry away again (true also when there was none).
pub fn remove_in(data_home: &Path) -> bool {
    let file = target_file(data_home);
    !file.is_file() || std::fs::remove_file(file).is_ok()
}

/// [`remove_in`] for this machine.
pub fn remove(host: &Host) -> bool {
    remove_in(&data_home(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;
    use crate::testdir::Dir;

    fn host(vars: &[(&str, &str)]) -> Host {
        Host::new(Flavor::Posix, vars, "")
    }

    /// A made-up program in a folder with a space in its name.
    fn program_in(dir: &Path) -> PathBuf {
        let folder = dir.join("my programs");
        std::fs::create_dir_all(&folder).unwrap();
        let program = folder.join("VerseKit.AppImage");
        std::fs::write(&program, b"x").unwrap();
        program
    }

    #[test]
    fn the_entry_has_the_lines_of_4_12_in_the_same_order_and_the_window_class_not_the_name() {
        let text = content("\"/a b/versekit\"", "/icons/versekit.png", "VerseKit", "Deutsch", "English");
        // The entry stands under games only.
        assert_eq!(
            text,
            "[Desktop Entry]\nType=Application\nName=VerseKit\nComment=Deutsch\nComment[en]=English\n\
             Exec=\"/a b/versekit\"\nIcon=/icons/versekit.png\nTerminal=false\nCategories=Game;\n\
             StartupWMClass=Versekit\nKeywords=Star Citizen;Blueprint;Bauplan;\n"
        );
        assert!(text.contains(&format!("StartupWMClass={WM_CLASS}\n")));
    }

    #[test]
    fn the_command_quotes_the_path_and_escapes_after_the_specification() {
        let none: &[String] = &[];
        assert_eq!(
            command(Path::new("/home/me/my programs/VerseKit.AppImage"), none),
            "\"/home/me/my programs/VerseKit.AppImage\""
        );
        assert_eq!(command(Path::new("/opt/100%/versekit"), none), "\"/opt/100%%/versekit\"");
        // In quotes a backslash goes before " ` $ \, then every backslash is doubled as in any string value.
        assert_eq!(command(Path::new("/opt/a\"b/versekit"), none), r#""/opt/a\\"b/versekit""#);
        assert_eq!(command(Path::new("/opt/$HOME/versekit"), none), r#""/opt/\\$HOME/versekit""#);
        assert_eq!(command(Path::new("/opt/a`b`/versekit"), none), r#""/opt/a\\`b\\`/versekit""#);
        assert_eq!(command(Path::new("/opt/a\\b/versekit"), none), r#""/opt/a\\\\b/versekit""#);
        assert_eq!(exec_argument("a\nb"), r#""a\nb""#, "a line break stays in its line");
        // Each argument after the program is quoted on its own.
        let copy = ["--daten".to_string(), "/home/me/Test kopie/50%".to_string()];
        assert_eq!(command(Path::new("/opt/v"), &copy), r#""/opt/v" "--daten" "/home/me/Test kopie/50%%""#);
    }

    #[test]
    fn the_command_for_a_shortcut_is_one_quoted_argument_without_doubled_percent_signs() {
        let none: &[String] = &[];
        assert_eq!(typed_command(Path::new("/home/a b/V.AppImage"), none), "\"/home/a b/V.AppImage\"");
        assert_eq!(typed_command(Path::new("/h/50%/$x/`y`/a\"b/c\\d"), none), r#""/h/50%/\$x/\`y\`/a\"b/c\\d""#);
        let copy = ["--daten".to_string(), "/home/me/$Test kopie".to_string()];
        assert_eq!(typed_command(Path::new("/opt/v"), &copy), r#""/opt/v" "--daten" "/home/me/\$Test kopie""#);
    }

    /// ⚠⚠ A start from the menu, the login or a shortcut of a run on a copy must land on that copy: without
    /// `--daten` it would run on the user's own data beside it.
    #[test]
    fn every_command_of_a_run_on_a_copy_names_the_copy_and_one_on_own_data_names_none() {
        let own = host(&[]);
        assert!(run_arguments(&own).is_empty(), "the user's own data: nothing after the program");
        let mut copy = host(&[]);
        copy.copy = Some("/home/me/VerseKit Testumgebung/Testkopie".to_string());
        let arguments = run_arguments(&copy);
        assert_eq!(arguments, ["--daten", "/home/me/VerseKit Testumgebung/Testkopie"]);
        let typed = typed_command(Path::new("/opt/V.AppImage"), &arguments);
        assert_eq!(typed, r#""/opt/V.AppImage" "--daten" "/home/me/VerseKit Testumgebung/Testkopie""#);
        // The entry in the menu of a copy: its Exec line starts the program and names the copy.
        let dir = Dir::new("menue-kopie");
        let program = program_in(dir.path());
        create_in(dir.path(), &program, &arguments, None).expect("written");
        let text = std::fs::read_to_string(target_file(dir.path())).unwrap();
        let exec = text.lines().find_map(|l| l.strip_prefix("Exec=")).unwrap();
        assert!(exec.ends_with(r#" "--daten" "/home/me/VerseKit Testumgebung/Testkopie""#), "{exec}");
        assert_eq!(exec_program(exec), program.to_string_lossy(), "the program is still the first argument");
        assert!(exists(dir.path()));
    }

    #[test]
    fn a_wayland_session_is_known_by_its_kind_or_its_display() {
        assert!(wayland(&host(&[("XDG_SESSION_TYPE", "wayland")])));
        assert!(wayland(&host(&[("XDG_SESSION_TYPE", "Wayland"), ("DISPLAY", ":0")])), "also with XWayland");
        assert!(wayland(&host(&[("WAYLAND_DISPLAY", "wayland-0")])));
        assert!(!wayland(&host(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")])));
        assert!(!wayland(&host(&[("WAYLAND_DISPLAY", "")])));
    }

    #[test]
    fn every_path_comes_back_out_of_its_command_unchanged() {
        for path in [
            "/home/a b/50% %/Ü's\"/V.AppImage",
            "/home/a b/$HOME/`id`/back\\slash/%f/V.AppImage",
            "/opt/versekit",
            "/a\\\\\"$`%%/x",
        ] {
            assert_eq!(exec_program(&exec_argument(path)), path, "{path}");
        }
        assert_eq!(exec_program("/usr/bin/versekit --flag"), "/usr/bin/versekit", "without quotes: up to the space");
    }

    #[test]
    fn an_entry_whose_path_has_a_dollar_a_backtick_and_a_percent_sign_still_counts_as_there() {
        let dir = Dir::new("menue-sonderzeichen");
        let folder = dir.path().join("a b $x `y` 50% Ü's");
        std::fs::create_dir_all(&folder).unwrap();
        let program = folder.join("VerseKit.AppImage");
        std::fs::write(&program, b"x").unwrap();
        create_in(dir.path(), &program, &[], None).expect("written");
        let text = std::fs::read_to_string(target_file(dir.path())).unwrap();
        assert!(text.contains(r"\\$x \\`y\\` 50%% "), "{text}");
        assert!(exists(dir.path()), "the escaped path leads to the program");
    }

    /// An entry as an older version wrote it: other words, an own icon file.
    fn old_entry(data: &Path, icon: &str) -> PathBuf {
        let file = target_file(data);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            format!(
                "[Desktop Entry]\nType=Application\nName=Verse-Kit\nComment=alt\nComment[en]=old\n\
                 Exec=\"/x/V.AppImage\"\nIcon={icon}\nStartupWMClass=Verse-Kit\nX-Eigenes=bleibt\n"
            ),
        )
        .unwrap();
        file
    }

    #[test]
    fn the_words_of_an_entry_follow_the_version_and_exec_icon_and_other_lines_stay() {
        let dir = Dir::new("menue-worte");
        let file = old_entry(dir.path(), "/eigenes/symbol.png");
        assert!(refresh_label(dir.path()));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(&format!("Name={}\n", language::t_in(Lang::De, "hf_titel"))), "{text}");
        assert!(text.contains(&format!("Comment[en]={}\n", language::t_in(Lang::En, "vk_untertitel"))));
        assert!(text.contains(&format!("StartupWMClass={WM_CLASS}\n")), "the window class, not the name");
        for kept in ["Exec=\"/x/V.AppImage\"\n", "Icon=/eigenes/symbol.png\n", "X-Eigenes=bleibt\n"] {
            assert!(text.contains(kept), "{kept} stays: {text}");
        }
        assert!(!refresh_label(dir.path()), "the same words: nothing written");
    }

    #[test]
    fn an_entry_that_was_deleted_is_not_made_again_by_following() {
        let dir = Dir::new("menue-geloescht");
        let program = program_in(dir.path());
        let icon = dir.path().join("icon.png");
        std::fs::write(&icon, b"neu").unwrap();
        assert!(!follow_in(dir.path(), &program, Some(&icon)));
        assert!(!target_file(dir.path()).exists(), "no entry out of nothing");
        assert!(!icon_dir(dir.path()).exists());
    }

    #[test]
    fn the_own_icon_file_follows_the_program_and_a_foreign_icon_stays() {
        let dir = Dir::new("menue-symbol-nach");
        let source = dir.path().join("icon.png");
        std::fs::write(&source, b"neu").unwrap();
        let own = icon_dir(dir.path()).join(ICON_FILE);
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, b"alt").unwrap();
        old_entry(dir.path(), &own.to_string_lossy());
        assert!(refresh_icon(dir.path(), Some(&source)));
        assert_eq!(std::fs::read(&own).unwrap(), b"neu");
        assert!(!refresh_icon(dir.path(), Some(&source)), "the same bytes: nothing written");
        // An icon of the user's own, or the name of a theme icon: untouched.
        let foreign = dir.path().join("mein.png");
        std::fs::write(&foreign, b"meins").unwrap();
        old_entry(dir.path(), &foreign.to_string_lossy());
        assert!(!refresh_icon(dir.path(), Some(&source)));
        assert_eq!(std::fs::read(&foreign).unwrap(), b"meins");
        old_entry(dir.path(), ICON_NAME);
        assert!(!refresh_icon(dir.path(), Some(&source)));
    }

    #[test]
    fn the_entry_of_4_12_is_taken_over_as_the_new_one_and_goes_with_its_icon() {
        let dir = Dir::new("menue-uebernahme");
        let program = program_in(dir.path());
        let old = menu_dir(dir.path()).join(legacy::MENU_FILE);
        let old_icon = icon_dir(dir.path()).join(legacy::MENU_ICON);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::create_dir_all(old_icon.parent().unwrap()).unwrap();
        std::fs::write(&old_icon, b"alt").unwrap();
        std::fs::write(&old, format!("[Desktop Entry]\nExec=\"/alt\"\nIcon={}\n", old_icon.to_string_lossy())).unwrap();
        assert!(follow_in(dir.path(), &program, None));
        assert!(!old.exists() && !old_icon.exists(), "the old entry and its icon are gone");
        assert!(exists(dir.path()), "the new one points to the running program");
        assert!(!take_over(dir.path(), &program, None), "nothing old: nothing to take over");
    }

    #[test]
    fn a_new_entry_that_is_there_stays_and_the_old_one_only_goes() {
        let dir = Dir::new("menue-uebernahme-beide");
        let program = program_in(dir.path());
        let file = old_entry(dir.path(), "/eigenes/symbol.png");
        let old = menu_dir(dir.path()).join(legacy::MENU_FILE);
        std::fs::write(&old, "[Desktop Entry]\nIcon=/eigenes/symbol.png\n").unwrap();
        assert!(take_over(dir.path(), &program, None));
        assert!(!old.exists());
        assert!(std::fs::read_to_string(file).unwrap().contains("Exec=\"/x/V.AppImage\""), "not written over");
    }

    #[test]
    fn the_old_entry_stays_when_the_new_one_cannot_be_written() {
        let dir = Dir::new("menue-uebernahme-fehler");
        let old = menu_dir(dir.path()).join(legacy::MENU_FILE);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "[Desktop Entry]\n").unwrap();
        assert!(!take_over(dir.path(), &dir.path().join("fehlt"), None));
        assert!(old.exists(), "the player keeps the entry they had");
    }

    #[test]
    fn the_program_is_the_appimage_while_it_is_a_file_and_never_the_unpacked_folder() {
        let dir = Dir::new("menue-programm");
        let image = program_in(dir.path());
        let running = Path::new("/tmp/.mount_VerseKxyz/versekit");
        let set = image.to_string_lossy().into_owned();
        assert_eq!(program(&host(&[("APPIMAGE", &set)]), running), image, "the AppImage wins");
        let gone = dir.path().join("weg.AppImage").to_string_lossy().into_owned();
        assert_eq!(program(&host(&[("APPIMAGE", &gone)]), running), running, "a file that is not there does not");
        assert_eq!(program(&host(&[("APPIMAGE", "")]), running), running, "an empty variable does not");
        assert_eq!(program(&host(&[]), running), running);
    }

    #[test]
    fn the_data_folder_is_xdg_data_home_or_the_local_share_of_the_home() {
        assert_eq!(data_home(&host(&[("XDG_DATA_HOME", "/x/data"), ("HOME", "/h")])), PathBuf::from("/x/data"));
        assert_eq!(
            data_home(&host(&[("XDG_DATA_HOME", ""), ("HOME", "/h")])),
            Path::new("/h").join(".local").join("share")
        );
        assert_eq!(
            target_file(Path::new("/d")),
            Path::new("/d").join("applications").join(FILE_NAME),
            "the entry lies in applications"
        );
    }

    #[test]
    fn writing_the_entry_puts_the_file_and_the_icon_and_the_entry_exists_then() {
        let dir = Dir::new("menue-anlegen");
        let data = dir.path().join("data");
        let program = program_in(dir.path());
        let icon = dir.path().join("icon.png");
        std::fs::write(&icon, b"png").unwrap();
        assert!(!exists(&data), "nothing yet");
        let file = create_in(&data, &program, &[], Some(&icon)).expect("written");
        assert_eq!(file, target_file(&data));
        let text = std::fs::read_to_string(&file).unwrap();
        let icon_target = icon_dir(&data).join(ICON_FILE);
        assert!(text.contains(&format!("Exec={}\n", command(&program, &[]))), "{text}");
        assert!(text.contains(&format!("Icon={}\n", icon_target.to_string_lossy())), "{text}");
        assert_eq!(std::fs::read(&icon_target).unwrap(), b"png", "the icon is copied");
        assert!(!file.with_extension("desktop.teil").exists(), "no temporary file stays");
        assert!(exists(&data), "the program is in a folder with a space: the whole path counts");
    }

    #[test]
    fn without_an_icon_to_copy_the_entry_names_the_icon_of_the_theme() {
        let dir = Dir::new("menue-ohne-symbol");
        let program = program_in(dir.path());
        let file = create_in(dir.path(), &program, &[], None).expect("written");
        assert!(std::fs::read_to_string(file).unwrap().contains("Icon=versekit\n"));
        assert!(!icon_dir(dir.path()).exists(), "nothing was copied");
    }

    #[test]
    fn an_entry_that_points_to_a_program_that_is_gone_does_not_count_as_there() {
        let dir = Dir::new("menue-tot");
        let program = program_in(dir.path());
        create_in(dir.path(), &program, &[], None).expect("written");
        assert!(exists(dir.path()));
        std::fs::remove_file(&program).unwrap();
        assert!(!exists(dir.path()), "the card is offered again");
        // A file without an Exec line counts as there, as in 4.12.
        std::fs::write(target_file(dir.path()), "[Desktop Entry]\nName=x\n").unwrap();
        assert!(exists(dir.path()));
    }

    #[test]
    fn a_program_that_is_not_there_gets_no_entry_and_the_fault_names_it() {
        let dir = Dir::new("menue-kein-programm");
        let missing = dir.path().join("fehlt");
        let said = create_in(dir.path(), &missing, &[], None).unwrap_err();
        assert_eq!(said, missing.to_string_lossy());
        assert!(!target_file(dir.path()).exists());
    }

    #[test]
    fn taking_the_entry_away_removes_the_file_and_is_fine_when_there_is_none() {
        let dir = Dir::new("menue-weg");
        let program = program_in(dir.path());
        assert!(remove_in(dir.path()), "none: fine");
        create_in(dir.path(), &program, &[], None).expect("written");
        assert!(remove_in(dir.path()));
        assert!(!target_file(dir.path()).exists());
    }

    #[test]
    fn the_icon_is_looked_for_in_the_appimage_and_next_to_the_program() {
        let dir = Dir::new("menue-symbolort");
        let exe = dir.path().join("bin").join("versekit");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        assert_eq!(icon_source(&host(&[]), &exe), None);
        let beside = exe.parent().unwrap().join("icon.png");
        std::fs::write(&beside, b"1").unwrap();
        assert_eq!(icon_source(&host(&[]), &exe), Some(beside));
        let appdir = dir.path().join("appdir");
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::write(appdir.join(ICON_FILE), b"2").unwrap();
        let set = appdir.to_string_lossy().into_owned();
        assert_eq!(icon_source(&host(&[("APPDIR", &set)]), &exe), Some(appdir.join(ICON_FILE)), "the AppImage first");
    }

    #[test]
    fn the_entry_is_offered_where_there_are_menus_of_desktop_files_only() {
        let menus = cfg!(not(any(windows, target_os = "macos")));
        assert_eq!(available(), menus);
        if !menus {
            assert!(create(&host(&[])).is_err(), "nothing is written where it is not offered");
        }
    }
}
