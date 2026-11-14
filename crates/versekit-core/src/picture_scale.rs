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

//! A part of a picture enlarged without the system, the same on every system: bicubic (Catmull-Rom, the centres
//! of the pixels half a pixel in, as GDI+ draws with `PixelOffsetModeHalf`) for enlarging, the mean of the
//! pixels covered for making smaller, and the colour matrices of [`crate::win_ocr::matrix`] applied to every
//! pixel. Under Windows GDI+ does it ([`crate::win_ocr::scaled`]); elsewhere this, and the own text recognition
//! uses it everywhere.
//!
//! BGRA in and out, top row first; the fourth byte is carried along unchanged by the matrices (they keep alpha).

/// A part (x, y, width, height) of a BGRA picture `width` × `height`, made `size` large, through `matrix`
/// (5 × 5, rows R, G, B, A, offset). `None` when the part does not lie in the picture or the bytes are too few.
pub fn scaled(
    raw: &[u8],
    width: usize,
    height: usize,
    area: (i64, i64, i64, i64),
    size: (usize, usize),
    matrix: Option<&[f32; 25]>,
) -> Option<Vec<u8>> {
    let (ax, ay, aw, ah) = area;
    let inside = ax >= 0 && ay >= 0 && aw > 0 && ah > 0 && ax + aw <= width as i64 && ay + ah <= height as i64;
    if !inside || raw.len() < width * height * 4 || size.0 == 0 || size.1 == 0 {
        return None;
    }
    let (ax, ay, aw, ah) = (ax as usize, ay as usize, aw as usize, ah as usize);
    let (tw, th) = size;
    let mut out = vec![0u8; tw * th * 4];
    let shrink = tw < aw || th < ah;
    if shrink {
        box_mean(raw, width, (ax, ay, aw, ah), size, &mut out);
    } else {
        bicubic(raw, width, (ax, ay, aw, ah), size, &mut out);
    }
    if let Some(m) = matrix {
        apply(&mut out, m);
    }
    Some(out)
}

/// The sharpness of the cubic kernel (Keys): -0.5 is Catmull-Rom.
const SHARPNESS: f32 = -0.5;

/// The weight of the cubic kernel at the distance `x`.
fn kernel(x: f32) -> f32 {
    let (a, x) = (SHARPNESS, x.abs());
    if x <= 1.0 {
        (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

/// The weights for the four neighbours at the distance `t` (0..1) behind the second.
fn weights(t: f32) -> [f32; 4] {
    [kernel(1.0 + t), kernel(t), kernel(1.0 - t), kernel(2.0 - t)]
}

/// For every target column (or row) the four source columns within the part and their weights.
fn taps(source: usize, target: usize) -> Vec<([usize; 4], [f32; 4])> {
    let ratio = source as f32 / target as f32;
    (0..target)
        .map(|i| {
            let at = (i as f32 + 0.5) * ratio - 0.5;
            let base = at.floor();
            let w = weights(at - base);
            let base = base as i64;
            let clamp = |k: i64| k.clamp(0, source as i64 - 1) as usize;
            ([clamp(base - 1), clamp(base), clamp(base + 1), clamp(base + 2)], w)
        })
        .collect()
}

fn bicubic(raw: &[u8], width: usize, area: (usize, usize, usize, usize), size: (usize, usize), out: &mut [u8]) {
    let (ax, ay, aw, ah) = area;
    let (tw, th) = size;
    let columns = taps(aw, tw);
    let rows = taps(ah, th);
    // First across: every source row of the part to the target width, four channels as floats.
    let mut across = vec![0f32; ah * tw * 4];
    for y in 0..ah {
        let line = &raw[((ay + y) * width + ax) * 4..((ay + y) * width + ax + aw) * 4];
        for (x, (at, w)) in columns.iter().enumerate() {
            for c in 0..4 {
                let v: f32 = (0..4).map(|k| f32::from(line[at[k] * 4 + c]) * w[k]).sum();
                across[(y * tw + x) * 4 + c] = v;
            }
        }
    }
    for (y, (at, w)) in rows.iter().enumerate() {
        for x in 0..tw {
            for c in 0..4 {
                let v: f32 = (0..4).map(|k| across[(at[k] * tw + x) * 4 + c] * w[k]).sum();
                out[(y * tw + x) * 4 + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn box_mean(raw: &[u8], width: usize, area: (usize, usize, usize, usize), size: (usize, usize), out: &mut [u8]) {
    let (ax, ay, aw, ah) = area;
    let (tw, th) = size;
    let span = |i: usize, source: usize, target: usize| {
        let from = i * source / target;
        let to = ((i + 1) * source).div_ceil(target).max(from + 1).min(source);
        from..to
    };
    for y in 0..th {
        let ys = span(y, ah, th);
        for x in 0..tw {
            let xs = span(x, aw, tw);
            let mut sum = [0u32; 4];
            for sy in ys.clone() {
                for sx in xs.clone() {
                    let p = ((ay + sy) * width + ax + sx) * 4;
                    for c in 0..4 {
                        sum[c] += u32::from(raw[p + c]);
                    }
                }
            }
            let n = (ys.len() * xs.len()) as u32;
            for c in 0..4 {
                out[(y * tw + x) * 4 + c] = ((sum[c] + n / 2) / n) as u8;
            }
        }
    }
}

/// The colour matrix on every pixel: out = (r, g, b, a, 1) · m, each channel 0..1.
fn apply(bgra: &mut [u8], m: &[f32; 25]) {
    for pixel in bgra.as_chunks_mut::<4>().0 {
        let input = [pixel[2], pixel[1], pixel[0], pixel[3]].map(|v| f32::from(v) / 255.0);
        let channel = |column: usize| {
            let v = (0..4).map(|row| input[row] * m[row * 5 + column]).sum::<f32>() + m[20 + column];
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let (r, g, b, a) = (channel(0), channel(1), channel(2), channel(3));
        *pixel = [b, g, r, a];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: usize, height: usize, value: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let mut raw = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let v = value(x, y);
                raw.extend_from_slice(&[v, v, v, 255]);
            }
        }
        raw
    }

    #[test]
    fn an_even_surface_stays_even_in_any_size() {
        let raw = picture(8, 6, |_, _| 77);
        for size in [(24, 18), (3, 2), (8, 6)] {
            let out = scaled(&raw, 8, 6, (0, 0, 8, 6), size, None).expect("scaled");
            assert!(out.as_chunks::<4>().0.iter().all(|p| p[..3] == [77, 77, 77]), "{size:?}");
        }
    }

    #[test]
    fn enlarging_keeps_a_bright_square_bright_inside_and_the_dark_round_it() {
        let raw = picture(8, 8, |x, y| if (2..6).contains(&x) && (2..6).contains(&y) { 230 } else { 20 });
        let out = scaled(&raw, 8, 8, (2, 2, 4, 4), (12, 12), None).expect("scaled");
        let at = |x: usize, y: usize| out[(y * 12 + x) * 4 + 1];
        assert!(at(6, 6) > 200, "inside {}", at(6, 6));
        // The part is taken alone: its edge does not blend with the picture round it.
        assert!(at(0, 0) > 200, "the part alone: {}", at(0, 0));
        let wide = scaled(&raw, 8, 8, (0, 0, 8, 8), (24, 24), None).expect("scaled");
        assert!(wide[(24 + 1) * 4 + 1] < 40, "dark round it: the point (1, 1)");
    }

    #[test]
    fn making_smaller_takes_the_mean() {
        let raw = picture(4, 2, |x, _| if x % 2 == 0 { 0 } else { 200 });
        let out = scaled(&raw, 4, 2, (0, 0, 4, 2), (2, 1), None).expect("scaled");
        assert_eq!((out[0], out[4]), (100, 100));
    }

    #[test]
    fn a_part_outside_the_picture_is_refused() {
        let raw = picture(4, 4, |_, _| 1);
        assert_eq!(scaled(&raw, 4, 4, (2, 2, 3, 2), (6, 4), None), None);
        assert_eq!(scaled(&raw, 4, 4, (0, 0, 4, 4), (0, 4), None), None);
        assert_eq!(scaled(&raw[..10], 4, 4, (0, 0, 4, 4), (8, 8), None), None);
    }

    #[test]
    fn the_dark_matrix_turns_bright_writing_dark() {
        let m = crate::win_ocr::matrix(crate::refinery_scan::Mode::Dark).expect("matrix");
        let raw = picture(2, 1, |x, _| if x == 0 { 230 } else { 20 });
        let out = scaled(&raw, 2, 1, (0, 0, 2, 1), (2, 1), Some(&m)).expect("scaled");
        assert!(out[1] < 60 && out[5] > 200, "{out:?}");
        assert_eq!(out[3], 255, "alpha kept");
    }
}
