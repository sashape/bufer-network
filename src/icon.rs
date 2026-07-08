//! Иконка приложения: синий скруглённый квадрат со стрелками туда-обратно.
//! Рисуется в коде (софтверная растеризация с суперсэмплингом) — как и в
//! Python-версии, никаких ресурсов в exe.

use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// Геометрия в базовых координатах 64x64 (как в _make_icon_image).
fn coverage(x: f32, y: f32) -> (f32, [f32; 3]) {
    const BLUE: [f32; 3] = [0.0, 103.0 / 255.0, 192.0 / 255.0];
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

    // белые стрелки поверх синего фона
    let in_line1 = (16.0..=44.0).contains(&x) && (23.0..=29.0).contains(&y);
    let in_tri1 = point_in_triangle(x, y, (44.0, 18.0), (56.0, 26.0), (44.0, 34.0));
    let in_line2 = (20.0..=48.0).contains(&x) && (39.0..=45.0).contains(&y);
    let in_tri2 = point_in_triangle(x, y, (20.0, 34.0), (8.0, 42.0), (20.0, 50.0));
    if in_line1 || in_tri1 || in_line2 || in_tri2 {
        return (1.0, WHITE);
    }
    // скруглённый квадрат [4,4,60,60] радиус 14
    if rounded_rect_contains(x, y, 4.0, 4.0, 60.0, 60.0, 14.0) {
        return (1.0, BLUE);
    }
    (0.0, BLUE)
}

fn rounded_rect_contains(x: f32, y: f32, l: f32, t: f32, r: f32, b: f32, rad: f32) -> bool {
    if x < l || x > r || y < t || y > b {
        return false;
    }
    let cx = x.clamp(l + rad, r - rad);
    let cy = y.clamp(t + rad, b - rad);
    (x - cx).powi(2) + (y - cy).powi(2) <= rad * rad
}

fn point_in_triangle(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let d1 = sign((px, py), a, b);
    let d2 = sign((px, py), b, c);
    let d3 = sign((px, py), c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

/// BGRA-пиксели (premultiplied) для иконки размера size.
fn rasterize(size: u32) -> Vec<u8> {
    const SS: u32 = 4; // суперсэмплинг 4x4
    let scale = 64.0 / size as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut a, mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * scale;
                    let (cov, rgb) = coverage(x, y);
                    a += cov;
                    r += rgb[0] * cov;
                    g += rgb[1] * cov;
                    b += rgb[2] * cov;
                }
            }
            let n = (SS * SS) as f32;
            // premultiplied alpha, BGRA
            out.push((b / n * 255.0) as u8);
            out.push((g / n * 255.0) as u8);
            out.push((r / n * 255.0) as u8);
            out.push((a / n * 255.0) as u8);
        }
    }
    out
}

pub fn create(size: u32) -> Option<HICON> {
    let pixels = rasterize(size);
    let mask = vec![0u8; (size * size / 8) as usize];
    unsafe {
        let color = CreateBitmap(size as i32, size as i32, 1, 32, Some(pixels.as_ptr() as _));
        let mono = CreateBitmap(size as i32, size as i32, 1, 1, Some(mask.as_ptr() as _));
        let info = ICONINFO {
            fIcon: true.into(),
            hbmMask: mono,
            hbmColor: color,
            ..Default::default()
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color);
        let _ = DeleteObject(mono);
        icon
    }
}
