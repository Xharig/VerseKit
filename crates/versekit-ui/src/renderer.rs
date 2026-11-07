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

//! How the windows of the program are drawn: by wgpu, never by OpenGL.
//!
//! ⚠ On Windows wgpu opens DX12 alone. With every interface open (its default) wgpu loads the Vulkan and the
//! OpenGL driver of the graphics card while it looks for an adapter, also when it draws by DX12 in the end; with
//! NVIDIA both are `nvoglv64.dll`, which crashes in some drivers before the first frame. DX12 always has an
//! adapter: the software renderer of Windows when no card offers one. The variable `WGPU_BACKEND` is not followed
//! on Windows. Elsewhere wgpu takes what it finds (Vulkan or Metal, else OpenGL ES) and follows `WGPU_BACKEND`.
//!
//! ⚠ On Windows every window opens through [`run_native`], which tries the [`Stage`]s of [`ladder`] one after the
//! other on a hidden window of its own before the first frame: adapter, device and surface are made, the surface is
//! configured and one picture presented. The first stage that does all of it draws the window, with the very
//! device that passed. A wgpu error of a configured surface otherwise ends the program by a panic ("Invalid
//! surface" in the Windows Sandbox, 31.10.). A stage that fails later on the real window is remembered in
//! [`STATE_FILE`] and tried last at the next start of the same version. When no stage works the program says so
//! and notes it in the crash file instead of ending without a word.

use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::Paths;

#[cfg(windows)]
mod dx12;

/// The renderer of every window of the program.
pub const RENDERER: eframe::Renderer = eframe::Renderer::Wgpu;

/// The graphics interfaces wgpu may open: DX12 on Windows, elsewhere the ones eframe opens.
pub fn backends() -> wgpu::Backends {
    if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY | wgpu::Backends::GL)
    }
}

/// A new wgpu instance on [`backends`]; wgpu chooses the adapter, a graphics card before the software renderer.
pub fn setup() -> WgpuSetupCreateNew {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = backends();
    setup
}

/// Makes the window drawn by [`RENDERER`] on [`setup`]; the other options stay.
pub fn apply(options: &mut eframe::NativeOptions) {
    options.renderer = RENDERER;
    options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup());
}

/// The interfaces a window with `options` may be drawn by; `None` when OpenGL draws it or the set-up is not one
/// of its own.
pub fn drawn_by(options: &eframe::NativeOptions) -> Option<wgpu::Backends> {
    match (&options.renderer, &options.wgpu_options.wgpu_setup) {
        (eframe::Renderer::Wgpu, WgpuSetup::CreateNew(setup)) => Some(setup.instance_descriptor.backends),
        _ => None,
    }
}

/// What a window shows of what lies behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Nothing: the main window and the small windows.
    Opaque,
    /// The game, through the transparent pixels: the overlay.
    Transparent,
}

/// The adapter a [`Stage`] draws on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    /// A graphics card.
    Hardware,
    /// The software renderer of Windows (WARP).
    Software,
}

/// How the picture of a [`Stage`] reaches the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Present {
    /// Through DirectComposition, the only way Windows composes the alpha of a DX12 picture.
    Composition,
    /// Straight into the window, opaque.
    Window,
}

/// One way of drawing a window on Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub adapter: Adapter,
    pub present: Present,
}

impl Stage {
    pub const ALL: [Stage; 4] = [
        Stage { adapter: Adapter::Hardware, present: Present::Composition },
        Stage { adapter: Adapter::Software, present: Present::Composition },
        Stage { adapter: Adapter::Hardware, present: Present::Window },
        Stage { adapter: Adapter::Software, present: Present::Window },
    ];

    /// The name of the stage in [`STATE_FILE`], the start trail and the crash file.
    pub fn id(self) -> &'static str {
        match (self.adapter, self.present) {
            (Adapter::Hardware, Present::Composition) => "grafikkarte-dcomp",
            (Adapter::Software, Present::Composition) => "software-dcomp",
            (Adapter::Hardware, Present::Window) => "grafikkarte-fenster",
            (Adapter::Software, Present::Window) => "software-fenster",
        }
    }

    /// The stage named `id`, if there is one.
    pub fn from_id(id: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Does the window show the transparent pixels as transparent?
    pub fn transparent(self) -> bool {
        self.present == Present::Composition
    }

    /// The presentation system of DX12 for the stage.
    pub fn swapchain(self) -> wgpu::Dx12SwapchainKind {
        match self.present {
            Present::Composition => wgpu::Dx12SwapchainKind::DxgiFromVisual,
            Present::Window => wgpu::Dx12SwapchainKind::DxgiFromHwnd,
        }
    }

    /// [`setup`] with the presentation system of the stage.
    pub fn setup(self) -> WgpuSetupCreateNew {
        let mut setup = setup();
        setup.instance_descriptor.backend_options.dx12.presentation_system = self.swapchain();
        setup
    }
}

/// The order of the adapters, the highest first: one whose surface takes premultiplied alpha, then a real one
/// before the software renderer of Windows, then DX12, then a discrete graphics card.
pub fn adapter_rank(blends: bool, kind: wgpu::DeviceType, backend: wgpu::Backend) -> (bool, bool, bool, bool) {
    (blends, kind != wgpu::DeviceType::Cpu, backend == wgpu::Backend::Dx12, kind == wgpu::DeviceType::DiscreteGpu)
}

/// The stages a window with `look` tries, the first first: a transparent window keeps its transparency on the
/// software renderer before it gives it up; an opaque one goes through DirectComposition only when no way straight
/// into the window works.
pub fn ladder(look: Look) -> Vec<Stage> {
    let [card_composed, software_composed, card, software] = Stage::ALL;
    match look {
        Look::Transparent => vec![card_composed, software_composed, card, software],
        Look::Opaque => vec![card, software, card_composed, software_composed],
    }
}

/// `ladder` with the stages of `failed` moved to its end, in their order.
pub fn order(ladder: &[Stage], failed: &[Stage]) -> Vec<Stage> {
    let (last, first): (Vec<Stage>, Vec<Stage>) = ladder.iter().partition(|s| failed.contains(s));
    first.into_iter().chain(last).collect()
}

/// A stage that did not work, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub stage: Stage,
    pub error: String,
}

/// What [`choose`] found: the stage that works, what it made, and the stages before it that did not.
#[derive(Debug)]
pub struct Chosen<T> {
    pub stage: Stage,
    pub made: T,
    pub failed: Vec<Attempt>,
}

/// Tries `stages` in order with `attempt`; the first that works is the one. Every stage that fails is in the
/// answer, also when none works.
pub fn choose<T>(
    stages: &[Stage],
    mut attempt: impl FnMut(Stage) -> Result<T, String>,
) -> Result<Chosen<T>, Vec<Attempt>> {
    let mut failed = Vec::new();
    for &stage in stages {
        match attempt(stage) {
            Ok(made) => return Ok(Chosen { stage, made, failed }),
            Err(error) => failed.push(Attempt { stage, error }),
        }
    }
    Err(failed)
}

/// The lines that name every failed attempt, for the crash file and the message.
pub fn attempt_lines(failed: &[Attempt]) -> Vec<String> {
    failed.iter().map(|a| format!("{}: {}", a.stage.id(), a.error.replace('\n', " | "))).collect()
}

/// The file in the folder `Intern` that remembers the stages of a version that failed on a real window, and the
/// stage the overlay draws with (for the hint at its opacity in the main window).
pub const STATE_FILE: &str = "grafik.json";
const KEY_VERSION: &str = "fassung";
const KEY_FAILED: &str = "gescheitert";
const KEY_OVERLAY: &str = "overlay";

fn read_state(paths: &Paths) -> Vec<(String, Value)> {
    match jsonio::read_json(&paths.app_file(STATE_FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Vec::new(),
    }
}

fn write_state(paths: &Paths, state: Vec<(String, Value)>) -> bool {
    let file = paths.app_file(STATE_FILE);
    let folder = paths.host.flavor.dirname(&file);
    let text = jsonio::dumps(&Value::Object(state), Some(2), false);
    jsonio::make_dirs(&folder).is_ok() && jsonio::write_text_atomic(&file, &text, jsonio::Newline::Native).is_ok()
}

fn get<'a>(state: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    state.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn set(state: &mut Vec<(String, Value)>, key: &str, value: Value) {
    jsonio::map_set(state, key, value);
}

/// The stages that failed on a real window in `version`; a file of another version counts for nothing.
pub fn failed_stages(paths: &Paths, version: &str) -> Vec<Stage> {
    let state = read_state(paths);
    if get(&state, KEY_VERSION).and_then(Value::as_str) != Some(version) {
        return Vec::new();
    }
    match get(&state, KEY_FAILED) {
        Some(Value::Array(ids)) => ids.iter().filter_map(Value::as_str).filter_map(Stage::from_id).collect(),
        _ => Vec::new(),
    }
}

/// Remembers that `stage` failed on a real window in `version`; true if it is written.
pub fn remember_failed(paths: &Paths, version: &str, stage: Stage) -> bool {
    let mut failed = failed_stages(paths, version);
    if !failed.contains(&stage) {
        failed.push(stage);
    }
    let mut state = read_state(paths);
    set(&mut state, KEY_VERSION, Value::str(version));
    set(&mut state, KEY_FAILED, Value::Array(failed.iter().map(|s| Value::str(s.id())).collect()));
    write_state(paths, state)
}

/// Remembers the stage the overlay draws with; nothing is written when it is the one remembered.
pub fn remember_overlay(paths: &Paths, stage: Stage) -> bool {
    let mut state = read_state(paths);
    if get(&state, KEY_OVERLAY).and_then(Value::as_str) == Some(stage.id()) {
        return true;
    }
    set(&mut state, KEY_OVERLAY, Value::str(stage.id()));
    write_state(paths, state)
}

/// Did the overlay last start without transparency (a stage without DirectComposition)?
pub fn overlay_opaque(paths: &Paths) -> bool {
    get(&read_state(paths), KEY_OVERLAY).and_then(Value::as_str).and_then(Stage::from_id).is_some_and(|s| !s.transparent())
}

/// The stage this process draws with on Windows; `None` before the first window or elsewhere.
pub fn chosen() -> Option<Stage> {
    #[cfg(windows)]
    {
        dx12::chosen()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Does this process draw its windows with transparent pixels? Elsewhere than on Windows the window system decides.
pub fn draws_transparent() -> bool {
    chosen().is_none_or(Stage::transparent)
}

/// Keeps every message box of [`run_native`] away and [`STATE_FILE`] as it is: for the self-check, which runs where
/// nobody could close a box and on no data of its own.
pub fn keep_quiet(on: bool) {
    QUIET.store(on, std::sync::atomic::Ordering::Relaxed);
}

static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Lets `stages` fail before they are tried: a counter check of the self-check.
pub fn refuse_for_check(stages: &[Stage]) {
    *REFUSED.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = stages.to_vec();
}

/// Is `stage` refused by [`refuse_for_check`]?
pub fn refused(stage: Stage) -> bool {
    REFUSED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(&stage)
}

static REFUSED: std::sync::Mutex<Vec<Stage>> = std::sync::Mutex::new(Vec::new());

/// `eframe::run_native` for every window of the program. On Windows, when wgpu draws the window, the stage is
/// chosen first ([`ladder`] of `look`, on a hidden window) and the window drawn with the device that passed; when
/// none works the program says so, notes it in the crash file and the error comes back.
pub fn run_native(
    app_name: &str,
    #[cfg_attr(not(windows), allow(unused_mut))] mut options: eframe::NativeOptions,
    #[cfg_attr(not(windows), allow(unused_variables))] look: Look,
    app_creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    #[cfg(windows)]
    if options.renderer == eframe::Renderer::Wgpu {
        match dx12::prepare(look, QUIET.load(std::sync::atomic::Ordering::Relaxed)) {
            Ok(existing) => options.wgpu_options.wgpu_setup = WgpuSetup::Existing(existing),
            Err(text) => return Err(eframe::Error::AppCreation(text.into())),
        }
    }
    eframe::run_native(app_name, options, app_creator)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The options eframe gives draw by wgpu as well, but with OpenGL and Vulkan among the interfaces; [`apply`]
    /// takes them out on Windows.
    #[test]
    fn apply_leaves_no_opengl() {
        let mut options = eframe::NativeOptions::default();
        let surface = options.wgpu_options.surface;
        apply(&mut options);
        let opened = drawn_by(&options).expect("drawn by wgpu");
        assert_eq!(opened, backends());
        if cfg!(windows) {
            assert_eq!(opened, wgpu::Backends::DX12);
            assert!(!opened.intersects(wgpu::Backends::GL | wgpu::Backends::VULKAN));
        }
        assert_eq!(options.wgpu_options.surface, surface, "the surface of eframe stays");
    }

    /// The software renderer and the window without DirectComposition stand in for what fails, before the program
    /// gives up: "Invalid surface" of the graphics card in the Windows Sandbox (31.10.).
    #[test]
    fn a_stage_that_fails_hands_on_to_the_next_one() {
        let [card_composed, software_composed, card, software] = Stage::ALL;
        let broken = |bad: &[Stage]| {
            let bad = bad.to_vec();
            move |stage: Stage| {
                if bad.contains(&stage) { Err(format!("Invalid surface ({})", stage.id())) } else { Ok(stage.id()) }
            }
        };

        let chosen = choose(&ladder(Look::Opaque), broken(&[card])).expect("the software renderer");
        assert_eq!(chosen.stage, software);
        assert_eq!(chosen.made, "software-fenster");
        assert_eq!(chosen.failed, [Attempt { stage: card, error: "Invalid surface (grafikkarte-fenster)".into() }]);

        // The overlay keeps its transparency on the software renderer, and gives it up only after that.
        let chosen = choose(&ladder(Look::Transparent), broken(&[card_composed])).expect("software renderer");
        assert_eq!((chosen.stage, chosen.stage.transparent()), (software_composed, true));
        let chosen = choose(&ladder(Look::Transparent), broken(&[card_composed, software_composed])).expect("opaque");
        assert_eq!((chosen.stage, chosen.stage.transparent()), (card, false));
        assert_eq!(chosen.failed.len(), 2);

        // Nothing works: every stage is named.
        let failed = choose(&ladder(Look::Transparent), broken(&Stage::ALL)).expect_err("nothing works");
        assert_eq!(failed.iter().map(|a| a.stage).collect::<Vec<_>>(), Stage::ALL);
        assert!(attempt_lines(&failed)[0].starts_with("grafikkarte-dcomp: Invalid surface"));

        // Nothing failed: the first stage, untouched.
        let chosen = choose(&ladder(Look::Opaque), broken(&[])).expect("the card");
        assert_eq!((chosen.stage, chosen.failed.len()), (card, 0));
    }

    #[test]
    fn the_opaque_windows_try_direct_composition_last_and_the_overlay_tries_it_first() {
        let opaque = ladder(Look::Opaque);
        let straight = opaque.iter().take_while(|s| s.swapchain() == wgpu::Dx12SwapchainKind::DxgiFromHwnd).count();
        assert_eq!((straight, opaque.len()), (2, 4), "{opaque:?}");
        assert_eq!(opaque[0].adapter, Adapter::Hardware);
        let transparent = ladder(Look::Transparent);
        assert_eq!(transparent[0].swapchain(), wgpu::Dx12SwapchainKind::DxgiFromVisual);
        assert_eq!(transparent[0].adapter, Adapter::Hardware);
        assert_eq!(transparent.len(), 4);
        for stage in Stage::ALL {
            assert_eq!(Stage::from_id(stage.id()), Some(stage));
            let setup = stage.setup();
            assert_eq!(setup.instance_descriptor.backends, backends());
            assert_eq!(setup.instance_descriptor.backend_options.dx12.presentation_system, stage.swapchain());
        }
    }

    #[test]
    fn a_stage_that_failed_on_a_real_window_comes_last() {
        let [card_composed, software_composed, card, software] = Stage::ALL;
        let ladder = ladder(Look::Transparent);
        assert_eq!(order(&ladder, &[]), ladder);
        assert_eq!(order(&ladder, &[card_composed]), [software_composed, card, software, card_composed]);
        assert_eq!(order(&ladder, &[card, card_composed]), [software_composed, software, card_composed, card]);
    }

    fn paths_in(name: &str) -> (Paths, versekit_core::testdir::Dir) {
        use versekit_core::paths::Host;
        let dir = versekit_core::testdir::Dir::new(&format!("grafik-{name}"));
        let text = dir.path().to_string_lossy().into_owned();
        let vars = [(versekit_core::legacy::ENV_HOME, text.as_str()), ("HOME", text.as_str()), ("USERPROFILE", text.as_str())];
        (Paths::new(Host::new(versekit_core::pypath::Flavor::native(), &vars, &text)), dir)
    }

    #[test]
    fn the_failed_stages_hold_for_their_version_only_and_the_overlay_stage_is_kept_apart() {
        let (paths, _dir) = paths_in("merken");
        let [card_composed, _, card, software] = Stage::ALL;
        assert!(failed_stages(&paths, "5.0.0").is_empty());
        assert!(!overlay_opaque(&paths), "nothing known: transparent");
        assert!(remember_failed(&paths, "5.0.0", card_composed));
        assert!(remember_failed(&paths, "5.0.0", card));
        assert!(remember_failed(&paths, "5.0.0", card), "twice is once");
        assert_eq!(failed_stages(&paths, "5.0.0"), [card_composed, card]);
        assert!(failed_stages(&paths, "5.0.1").is_empty(), "a new version tries everything again");
        assert!(remember_overlay(&paths, software));
        assert!(overlay_opaque(&paths));
        assert_eq!(failed_stages(&paths, "5.0.0"), [card_composed, card], "the overlay stage leaves them");
        assert!(remember_overlay(&paths, card_composed));
        assert!(!overlay_opaque(&paths));
        // A new version starts the list anew.
        assert!(remember_failed(&paths, "5.0.1", software));
        assert_eq!(failed_stages(&paths, "5.0.1"), [software]);
        assert!(!overlay_opaque(&paths), "the overlay stage stays");
    }
}
