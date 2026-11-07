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

//! The text recognition of Windows for the refinery scanner (`win_ocr.py`): a part of the grabbed picture is
//! enlarged by GDI+ (bicubic, optionally through a colour matrix), handed to `Windows.Media.Ocr` as a
//! `SoftwareBitmap`, and the words come back with their frames in pixels of the picture.
//!
//! ⚠ The enlarging is the one of 4.12 to the last setting (pixel format, interpolation, pixel offset, the
//! matrices): the learned patterns of the digits were cut from cells enlarged so, and a cell enlarged otherwise
//! would lie off them.
//!
//! ⚠ The recognition needs a language installed for it (it comes with a language pack). First the languages of
//! the profile, else the first one Windows has; without any the reading ends with [`Reason::NoLanguage`].

use crate::refinery_scan::{Area, Job, Mode, Ocr, Picture, Reason, ScanError, Word};

/// A picture as the grab gives it: BGRA, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub width: i64,
    pub height: i64,
    pub raw: Vec<u8>,
}

impl Picture for Bitmap {
    fn size(&self) -> (i64, i64) {
        (self.width, self.height)
    }

    fn scaled(&self, area: Area, scale: f64, mode: Mode) -> Option<(usize, usize, Vec<u8>)> {
        scaled(self, area, scale, mode).ok()
    }
}

/// Is there the text recognition of Windows on this system?
pub fn supported() -> bool {
    cfg!(windows)
}

/// The colour matrix 5 × 5 of a mode, rows R, G, B, A, offset, columns the channels out (`_matrix`). `Gray`: grey
/// with the contrast spread, bright writing stays bright. `Dark`: grey turned round and spread, dark writing on a
/// bright ground. `Plain` has none.
pub fn matrix(mode: Mode) -> Option<[f32; 25]> {
    let (r, g, b, offset) = match mode {
        Mode::Plain => return None,
        Mode::Gray => (0.30 * 1.8, 0.59 * 1.8, 0.11 * 1.8, -0.25),
        Mode::Dark => (-0.30 / 0.7, -0.59 / 0.7, -0.11 / 0.7, 1.0 / 0.7),
    };
    let mut m = [[0.0f64; 5]; 5];
    for (row, value) in [(0, r), (1, g), (2, b), (4, offset)] {
        m[row][..3].fill(value);
    }
    m[3][3] = 1.0;
    m[4][4] = 1.0;
    let mut out = [0.0f32; 25];
    for (slot, value) in out.iter_mut().zip(m.iter().flatten()) {
        *slot = *value as f32;
    }
    Some(out)
}

/// `scaled`: the part `area` of `bitmap` enlarged by `scale`, bicubic by GDI+, through the matrix of `mode`.
/// Width, height and BGRA.
pub fn scaled(bitmap: &Bitmap, area: Area, scale: f64, mode: Mode) -> Result<(usize, usize, Vec<u8>), String> {
    let tw = ((area.2 as f64 * scale) as i64).max(1);
    let th = ((area.3 as f64 * scale) as i64).max(1);
    #[cfg(windows)]
    {
        imp::scaled(bitmap, area, (tw, th), matrix(mode))
    }
    #[cfg(not(windows))]
    {
        let _ = (bitmap, mode, tw, th);
        Err("GDI+ only under Windows".into())
    }
}

/// `png_bytes`: BGRA (top row first) as an RGB PNG.
pub fn png_bytes(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * 3 + 1) * height);
    for y in 0..height {
        raw.push(0);
        for pixel in bgra[y * width * 4..(y + 1) * width * 4].as_chunks::<4>().0 {
            raw.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut block = kind.to_vec();
        block.extend_from_slice(body);
        out.extend_from_slice(&block);
        out.extend_from_slice(&crate::zip::crc32(&block).to_be_bytes());
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// `save_png`: the part `area` as PNG, at most `max_width` wide (`None`: as it is).
pub fn png_of(bitmap: &Bitmap, area: Area, max_width: Option<i64>) -> Result<Vec<u8>, String> {
    let scale = match max_width {
        Some(most) if area.2 > most => most as f64 / area.2 as f64,
        _ => 1.0,
    };
    let (tw, th, raw) = scaled(bitmap, area, scale, Mode::Plain)?;
    Ok(png_bytes(tw, th, &raw))
}

/// The text recognition over one picture (`ocr_image`), for [`crate::refinery_scan::read_image`].
pub struct Recognition<'a> {
    pub bitmap: &'a Bitmap,
    /// The longest one reading may take.
    pub timeout: std::time::Duration,
}

impl<'a> Recognition<'a> {
    pub fn new(bitmap: &'a Bitmap) -> Recognition<'a> {
        Recognition { bitmap, timeout: std::time::Duration::from_secs(180) }
    }
}

/// A job made fit for the picture and the recognition: the part within the picture (or `None` when nothing of
/// it lies inside) and the enlargement, at most so far that the longer side stays within `limit`.
pub fn fitted(job: &Job, width: i64, height: i64, limit: u32) -> Option<(Area, f64)> {
    let (x, y) = (job.area.0.max(0), job.area.1.max(0));
    let (w, h) = (job.area.2.min(width - x), job.area.3.min(height - y));
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut scale = if job.scale > 0.0 { job.scale } else { 1.0 };
    let longer = w.max(h) as f64;
    if longer * scale > f64::from(limit) {
        scale = f64::from(limit) / longer;
    }
    Some(((x, y, w, h), scale))
}

/// A word of the recognition (frame in pixels of the enlarged part) in pixels of the picture.
pub fn word_in_picture(text: &str, frame: (f32, f32, f32, f32), area: Area, enlarged: (usize, usize)) -> Word {
    let sx = enlarged.0 as f64 / area.2 as f64;
    let sy = enlarged.1 as f64 / area.3 as f64;
    let (rx, ry, rw, rh) = (f64::from(frame.0), f64::from(frame.1), f64::from(frame.2), f64::from(frame.3));
    Word::new(
        text,
        (area.0 as f64 + rx / sx).trunc(),
        (area.1 as f64 + ry / sy).trunc(),
        (rw / sx).trunc(),
        (rh / sy).trunc(),
    )
}

impl Ocr for Recognition<'_> {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        #[cfg(windows)]
        {
            imp::read(self.bitmap, jobs, self.timeout)
        }
        #[cfg(not(windows))]
        {
            let _ = jobs;
            Err(ScanError::new(Reason::Unsupported, ""))
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows_sys::Win32::Graphics::GdiPlus::{
        ColorMatrix, GdipCreateBitmapFromScan0, GdipCreateImageAttributes, GdipDeleteGraphics, GdipDisposeImage,
        GdipDisposeImageAttributes, GdipDrawImageRectRectI, GdipGetImageGraphicsContext,
        GdipSetImageAttributesColorMatrix, GdipSetInterpolationMode, GdipSetPixelOffsetMode, GdiplusStartup,
        GdiplusStartupInput, GpBitmap, GpGraphics, GpImage, GpImageAttributes,
    };

    use super::{Area, Bitmap, Job, Reason, ScanError, Word, fitted, word_in_picture};

    /// `PixelFormat32bppRGB`.
    const RGB32: i32 = 0x0002_2009;
    /// `InterpolationModeHighQualityBicubic`.
    const BICUBIC: i32 = 7;
    /// The pixel offset 4.12 passes (`PixelOffsetModeHalf`).
    const PIXEL_OFFSET: i32 = 4;
    /// `UnitPixel`.
    const PIXELS: i32 = 2;

    /// GDI+ started once for the life of the process, as 4.12 does; never shut down.
    fn started() -> Result<(), String> {
        static STATUS: OnceLock<i32> = OnceLock::new();
        let status = *STATUS.get_or_init(|| {
            let mut token = 0usize;
            let input = GdiplusStartupInput {
                GdiplusVersion: 1,
                DebugEventCallback: 0,
                SuppressBackgroundThread: 0,
                SuppressExternalCodecs: 0,
            };
            // SAFETY: `token` and `input` live during the call; no output is asked for.
            unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) }
        });
        if status == 0 { Ok(()) } else { Err(format!("GdiplusStartup {status}")) }
    }

    fn ok(status: i32, what: &str) -> Result<(), String> {
        if status == 0 { Ok(()) } else { Err(format!("{what} {status}")) }
    }

    pub fn scaled(
        bitmap: &Bitmap,
        area: Area,
        size: (i64, i64),
        matrix: Option<[f32; 25]>,
    ) -> Result<(usize, usize, Vec<u8>), String> {
        started()?;
        let (width, height) = (bitmap.width as i32, bitmap.height as i32);
        if bitmap.raw.len() < (bitmap.width * bitmap.height * 4) as usize {
            return Err("Bild zu kurz".into());
        }
        let (tw, th) = (size.0 as i32, size.1 as i32);
        let mut target_buffer = vec![0u8; size.0 as usize * size.1 as usize * 4];
        let mut source: *mut GpBitmap = std::ptr::null_mut();
        let mut target: *mut GpBitmap = std::ptr::null_mut();
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        let mut attributes: *mut GpImageAttributes = std::ptr::null_mut();
        // SAFETY: the two buffers outlive the bitmaps made on them, which are disposed below before the target
        // buffer is read; every object made is disposed in the reverse order.
        let result = unsafe {
            let drawn = (|| {
                ok(
                    GdipCreateBitmapFromScan0(width, height, width * 4, RGB32, bitmap.raw.as_ptr(), &mut source),
                    "Bitmap",
                )?;
                ok(
                    GdipCreateBitmapFromScan0(tw, th, tw * 4, RGB32, target_buffer.as_mut_ptr(), &mut target),
                    "Bitmap",
                )?;
                ok(GdipGetImageGraphicsContext(target.cast::<GpImage>(), &mut graphics), "Graphics")?;
                GdipSetInterpolationMode(graphics, BICUBIC);
                GdipSetPixelOffsetMode(graphics, PIXEL_OFFSET);
                if let Some(values) = matrix {
                    ok(GdipCreateImageAttributes(&mut attributes), "ImageAttributes")?;
                    let m = ColorMatrix { m: values };
                    ok(GdipSetImageAttributesColorMatrix(attributes, 0, 1, &m, std::ptr::null(), 0), "ColorMatrix")?;
                }
                ok(
                    GdipDrawImageRectRectI(
                        graphics,
                        source.cast::<GpImage>(),
                        0,
                        0,
                        tw,
                        th,
                        area.0 as i32,
                        area.1 as i32,
                        area.2 as i32,
                        area.3 as i32,
                        PIXELS,
                        attributes,
                        0,
                        std::ptr::null_mut(),
                    ),
                    "DrawImage",
                )
            })();
            if !graphics.is_null() {
                GdipDeleteGraphics(graphics);
            }
            if !attributes.is_null() {
                GdipDisposeImageAttributes(attributes);
            }
            if !target.is_null() {
                GdipDisposeImage(target.cast::<GpImage>());
            }
            if !source.is_null() {
                GdipDisposeImage(source.cast::<GpImage>());
            }
            drawn
        };
        result.map(|()| (size.0 as usize, size.1 as usize, target_buffer))
    }

    fn failed(error: impl std::fmt::Display) -> ScanError {
        ScanError::new(Reason::Ocr, error.to_string())
    }

    /// The engine of the languages of the profile, else of the first language Windows has, with the longest side
    /// it takes.
    fn engine() -> Result<(OcrEngine, u32), ScanError> {
        let limit = OcrEngine::MaxImageDimension().map_err(failed)?;
        if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
            return Ok((engine, limit));
        }
        let languages = OcrEngine::AvailableRecognizerLanguages().map_err(failed)?;
        if languages.Size().map_err(failed)? == 0 {
            return Err(ScanError::new(Reason::NoLanguage, ""));
        }
        let first = languages.GetAt(0).map_err(failed)?;
        let engine = OcrEngine::TryCreateFromLanguage(&first).map_err(|_| ScanError::new(Reason::NoLanguage, ""))?;
        Ok((engine, limit))
    }

    /// A word of the recognition: text and frame (x, y, width, height) in pixels of the enlarged part.
    type Found = (String, (f32, f32, f32, f32));

    /// The words of one enlarged part.
    fn recognize(
        engine: &OcrEngine,
        size: (usize, usize),
        bgra: &[u8],
        timeout: Duration,
    ) -> Result<Vec<Found>, ScanError> {
        let buffer = CryptographicBuffer::CreateFromByteArray(bgra).map_err(failed)?;
        let picture = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            size.0 as i32,
            size.1 as i32,
            BitmapAlphaMode::Ignore,
        )
        .map_err(failed)?;
        let operation = engine.RecognizeAsync(&picture).map_err(failed)?;
        let end = Instant::now() + timeout;
        loop {
            match operation.Status().map_err(failed)?.0 {
                1 => break,
                state @ (2 | 3) => return Err(ScanError::new(Reason::Ocr, format!("recognition stopped ({state})"))),
                _ if Instant::now() > end => return Err(ScanError::new(Reason::Ocr, "timeout")),
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let result = operation.GetResults().map_err(failed)?;
        let _ = picture.Close();
        let mut words = Vec::new();
        for line in result.Lines().map_err(failed)? {
            for word in line.Words().map_err(failed)? {
                let rect = word.BoundingRect().map_err(failed)?;
                let text = word.Text().map_err(failed)?.to_string_lossy();
                words.push((text, (rect.X, rect.Y, rect.Width, rect.Height)));
            }
        }
        Ok(words)
    }

    pub fn read(bitmap: &Bitmap, jobs: &[Job], timeout: Duration) -> Result<Vec<Vec<Word>>, ScanError> {
        let (engine, limit) = engine()?;
        let mut passes = Vec::new();
        for job in jobs {
            let Some((area, scale)) = fitted(job, bitmap.width, bitmap.height, limit) else { continue };
            let (tw, th, raw) = super::scaled(bitmap, area, scale, job.mode).map_err(failed)?;
            let found = recognize(&engine, (tw, th), &raw, timeout)?;
            passes.push(found.iter().map(|(text, frame)| word_in_picture(text, *frame, area, (tw, th))).collect());
        }
        Ok(passes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrices_turn_grey_and_round_and_plain_has_none() {
        assert_eq!(matrix(Mode::Plain), None);
        let dark = matrix(Mode::Dark).expect("dark");
        assert_eq!(dark[0], (-0.30f64 / 0.7) as f32);
        assert_eq!(dark[20], (1.0f64 / 0.7) as f32);
        assert_eq!((dark[18], dark[24], dark[3]), (1.0, 1.0, 0.0));
        let gray = matrix(Mode::Gray).expect("gray");
        assert_eq!(gray[6], (0.59f64 * 1.8) as f32);
        assert_eq!(gray[22], -0.25);
    }

    #[test]
    fn a_job_is_cut_to_the_picture_and_kept_within_the_limit_of_the_recognition() {
        let job = |area, scale| Job { area, scale, mode: Mode::Plain };
        assert_eq!(fitted(&job((-10, 5, 100, 50), 3.0), 1000, 800, 10000), Some(((0, 5, 100, 50), 3.0)));
        assert_eq!(fitted(&job((950, 790, 100, 50), 3.0), 1000, 800, 10000), Some(((950, 790, 50, 10), 3.0)));
        assert_eq!(fitted(&job((1000, 0, 10, 10), 3.0), 1000, 800, 10000), None);
        assert_eq!(fitted(&job((0, 0, 4000, 100), 4.0), 5000, 800, 10000), Some(((0, 0, 4000, 100), 2.5)));
        assert_eq!(fitted(&job((0, 0, 10, 10), 0.0), 100, 100, 10000).map(|f| f.1), Some(1.0));
    }

    #[test]
    fn a_word_of_the_enlarged_part_stands_in_pixels_of_the_picture() {
        let word = word_in_picture("553", (300.5, 61.0, 54.9, 27.0), (2200, 540, 300, 450), (900, 1350));
        assert_eq!((word.x, word.y, word.w, word.h), (2300.0, 560.0, 18.0, 9.0));
        assert_eq!(word.t, "553");
    }

    #[test]
    fn a_png_carries_the_picture_in_rgb() {
        let png = png_bytes(2, 1, &[1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(png[24..26], [8, 2]);
        let start = 33 + 8;
        let length = u32::from_be_bytes(png[33..37].try_into().expect("length")) as usize;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&png[start..start + length]).expect("zlib");
        assert_eq!(raw, [0, 3, 2, 1, 6, 5, 4]);
    }

    #[cfg(windows)]
    #[test]
    fn gdi_plus_enlarges_a_part_and_the_dark_matrix_turns_it_round() {
        let mut raw = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                let v = if (2..6).contains(&x) && (2..6).contains(&y) { 230 } else { 20 };
                raw.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let bitmap = Bitmap { width: 8, height: 8, raw };
        let (w, h, plain) = scaled(&bitmap, (2, 2, 4, 4), 3.0, Mode::Plain).expect("plain");
        assert_eq!((w, h), (12, 12));
        let middle = (6 * 12 + 6) * 4;
        assert!(plain[middle + 1] > 200, "bright inside: {}", plain[middle + 1]);
        let (_, _, dark) = scaled(&bitmap, (2, 2, 4, 4), 3.0, Mode::Dark).expect("dark");
        assert!(dark[middle + 1] < 60, "turned round: {}", dark[middle + 1]);
    }
}
