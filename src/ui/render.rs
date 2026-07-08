//! Ручная отрисовка интерфейса через Direct2D + DirectWrite.
//! Никаких библиотек виджетов: карточки, кнопки и журнал рисуются примитивами,
//! текст — DirectWrite с цветными эмодзи. Всё в логических координатах (DIP),
//! масштаб под DPI берёт на себя render target (SetDpi).

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F,
    D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
    IDWriteTextLayout,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;

pub const D2DERR_RECREATE_TARGET: windows::core::HRESULT =
    windows::core::HRESULT(0x8899000Cu32 as i32);

#[derive(Clone, Copy, PartialEq)]
pub struct Rect {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Rect {
    pub fn new(l: f32, t: f32, r: f32, b: f32) -> Rect {
        Rect { l, t, r, b }
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
    pub fn w(&self) -> f32 {
        self.r - self.l
    }
    pub fn h(&self) -> f32 {
        self.b - self.t
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.l, top: self.t, right: self.r, bottom: self.b }
    }
}

pub fn rgb(hex: u32) -> D2D1_COLOR_F {
    rgba(hex, 1.0)
}

pub fn rgba(hex: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a,
    }
}

pub struct Theme {
    pub bg: D2D1_COLOR_F,
    pub card: D2D1_COLOR_F,
    pub border: D2D1_COLOR_F,
    pub hover: D2D1_COLOR_F,
    pub press: D2D1_COLOR_F,
    pub sel_bg: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub accent_hover: D2D1_COLOR_F,
    pub accent_press: D2D1_COLOR_F,
    pub accent_text: D2D1_COLOR_F,
    pub text: D2D1_COLOR_F,
    pub muted: D2D1_COLOR_F,
    pub log_bg: D2D1_COLOR_F,
    pub scrollbar: D2D1_COLOR_F,
}

pub fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            bg: rgb(0x1c1c1c),
            card: rgb(0x2b2b2b),
            border: rgb(0x3a3a3a),
            hover: rgb(0x333333),
            press: rgb(0x2e2e2e),
            sel_bg: rgba(0x4cc2ff, 0.16),
            accent: rgb(0x4cc2ff),
            accent_hover: rgb(0x45b3ec),
            accent_press: rgb(0x3ea2d6),
            accent_text: rgb(0x08202f),
            text: rgb(0xf2f2f2),
            muted: rgb(0x9a9a9a),
            log_bg: rgb(0x1f1f1f),
            scrollbar: rgb(0x5c5c5c),
        }
    } else {
        Theme {
            bg: rgb(0xfafafa),
            card: rgb(0xffffff),
            border: rgb(0xe0e0e0),
            hover: rgb(0xf4f4f4),
            press: rgb(0xededed),
            sel_bg: rgba(0x0067c0, 0.10),
            accent: rgb(0x0067c0),
            accent_hover: rgb(0x1573c8),
            accent_press: rgb(0x2280cf),
            accent_text: rgb(0xffffff),
            text: rgb(0x1b1b1b),
            muted: rgb(0x6b6b6b),
            log_bg: rgb(0xfdfdfd),
            scrollbar: rgb(0xbdbdbd),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

pub struct Ctx {
    _d2d: ID2D1Factory,
    pub dwrite: IDWriteFactory,
    hwnd: HWND,
    hwnd_rt: Option<ID2D1HwndRenderTarget>,
    rt: Option<ID2D1RenderTarget>, // тот же объект, базовый интерфейс для рисования
    brush: Option<ID2D1SolidColorBrush>,
    // ключ: размер*10 в старших битах + вес; форматы переживают кадры
    formats: RefCell<HashMap<u64, IDWriteTextFormat>>,
    icon_formats: RefCell<HashMap<u32, IDWriteTextFormat>>, // ключ: размер*10
    icon_family: Vec<u16>, // "Segoe Fluent Icons" (Win11) или "Segoe MDL2 Assets" (Win10)
}

impl Ctx {
    pub fn new(hwnd: HWND) -> windows::core::Result<Ctx> {
        unsafe {
            let d2d: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let icon_family = detect_icon_family(&dwrite);
            Ok(Ctx {
                _d2d: d2d,
                dwrite,
                hwnd,
                hwnd_rt: None,
                rt: None,
                brush: None,
                formats: RefCell::new(HashMap::new()),
                icon_formats: RefCell::new(HashMap::new()),
                icon_family,
            })
        }
    }

    /// Render target пересоздаётся лениво (после device lost) и живёт между кадрами.
    pub fn ensure_rt(&mut self, px_w: u32, px_h: u32, dpi: f32) -> windows::core::Result<()> {
        if self.rt.is_none() {
            unsafe {
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_UNKNOWN,
                        alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                    },
                    ..Default::default()
                };
                let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd: self.hwnd,
                    pixelSize: D2D_SIZE_U { width: px_w, height: px_h },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                };
                let hwnd_rt = self._d2d.CreateHwndRenderTarget(&props, &hwnd_props)?;
                let rt: ID2D1RenderTarget = hwnd_rt.cast()?;
                rt.SetDpi(dpi, dpi);
                self.brush = Some(rt.CreateSolidColorBrush(&rgb(0), None)?);
                self.hwnd_rt = Some(hwnd_rt);
                self.rt = Some(rt);
            }
        }
        Ok(())
    }

    pub fn resize(&self, px_w: u32, px_h: u32) {
        if let Some(rt) = &self.hwnd_rt {
            unsafe {
                let _ = rt.Resize(&D2D_SIZE_U { width: px_w, height: px_h });
            }
        }
    }

    pub fn set_dpi(&self, dpi: f32) {
        if let Some(rt) = &self.rt {
            unsafe { rt.SetDpi(dpi, dpi) };
        }
    }

    pub fn drop_rt(&mut self) {
        self.hwnd_rt = None;
        self.rt = None;
        self.brush = None;
    }

    pub fn begin(&self, bg: D2D1_COLOR_F) {
        if let Some(rt) = &self.rt {
            unsafe {
                rt.BeginDraw();
                rt.Clear(Some(&bg));
            }
        }
    }

    /// Завершение кадра; при потере устройства render target пересоздастся.
    pub fn end(&mut self) {
        let Some(rt) = &self.rt else { return };
        let result = unsafe { rt.EndDraw(None, None) };
        if let Err(e) = result {
            if e.code() == D2DERR_RECREATE_TARGET {
                self.drop_rt();
            }
        }
    }

    fn brush(&self, color: D2D1_COLOR_F) -> ID2D1SolidColorBrush {
        let b = self.brush.clone().unwrap();
        unsafe { b.SetColor(&color) };
        b
    }

    pub fn fill_round(&self, rect: Rect, radius: f32, color: D2D1_COLOR_F) {
        if let Some(rt) = &self.rt {
            let rr = D2D1_ROUNDED_RECT { rect: rect.d2d(), radiusX: radius, radiusY: radius };
            unsafe { rt.FillRoundedRectangle(&rr, &self.brush(color)) };
        }
    }

    pub fn stroke_round(&self, rect: Rect, radius: f32, color: D2D1_COLOR_F, width: f32) {
        if let Some(rt) = &self.rt {
            let rr = D2D1_ROUNDED_RECT { rect: rect.d2d(), radiusX: radius, radiusY: radius };
            unsafe { rt.DrawRoundedRectangle(&rr, &self.brush(color), width, None) };
        }
    }

    pub fn push_clip(&self, rect: Rect) {
        if let Some(rt) = &self.rt {
            unsafe { rt.PushAxisAlignedClip(&rect.d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        }
    }

    pub fn pop_clip(&self) {
        if let Some(rt) = &self.rt {
            unsafe { rt.PopAxisAlignedClip() };
        }
    }

    fn format(&self, size: f32, weight: u32, wrap: bool) -> IDWriteTextFormat {
        let key = ((size * 10.0) as u64) << 32 | (weight as u64) << 1 | wrap as u64;
        if let Some(f) = self.formats.borrow().get(&key) {
            return f.clone();
        }
        let f = unsafe {
            // Segoe UI Variable есть на Windows 11; на 10 DirectWrite сам
            // подставит запасной шрифт
            let f = self
                .dwrite
                .CreateTextFormat(
                    w!("Segoe UI Variable Text"),
                    None,
                    DWRITE_FONT_WEIGHT(weight as i32),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!(""),
                )
                .or_else(|_| {
                    self.dwrite.CreateTextFormat(
                        w!("Segoe UI"),
                        None,
                        DWRITE_FONT_WEIGHT(weight as i32),
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        w!(""),
                    )
                })
                .unwrap();
            let _ = f.SetWordWrapping(if wrap {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            });
            // однострочный текст, который не влезает, обрезаем многоточием,
            // а не жёстко по краю (кнопки, имена компьютеров и т.п.)
            if !wrap {
                if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&f) {
                    let trimming = DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    };
                    let _ = f.SetTrimming(&trimming, &sign);
                }
            }
            f
        };
        self.formats.borrow_mut().insert(key, f.clone());
        f
    }

    fn icon_format(&self, size: f32) -> IDWriteTextFormat {
        let key = (size * 10.0) as u32;
        if let Some(f) = self.icon_formats.borrow().get(&key) {
            return f.clone();
        }
        let f = unsafe {
            let f = self
                .dwrite
                .CreateTextFormat(
                    PCWSTR(self.icon_family.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT(400),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!(""),
                )
                .unwrap();
            let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            f
        };
        self.icon_formats.borrow_mut().insert(key, f.clone());
        f
    }

    /// Иконка-глиф из Segoe Fluent Icons, отцентрованная в прямоугольнике.
    pub fn icon(&self, codepoint: u32, rect: Rect, size: f32, color: D2D1_COLOR_F) {
        let Some(rt) = &self.rt else { return };
        let Some(ch) = char::from_u32(codepoint) else { return };
        let wide: Vec<u16> = ch.to_string().encode_utf16().collect();
        let format = self.icon_format(size);
        unsafe {
            rt.DrawText(
                &wide,
                &format,
                &rect.d2d(),
                &self.brush(color),
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// Иконка + подпись, отцентрованные как группа в прямоугольнике (для кнопок).
    pub fn icon_label(
        &self,
        glyph: u32,
        label: &str,
        rect: Rect,
        size: f32,
        weight: u32,
        color: D2D1_COLOR_F,
    ) {
        let text_w = self.measure_width(label, size, weight);
        let icon_w = size + 3.0;
        let gap = 7.0;
        let total = icon_w + gap + text_w;
        let start = (rect.l + (rect.w() - total) / 2.0).max(rect.l);
        self.icon(glyph, Rect::new(start, rect.t, start + icon_w, rect.b), size + 3.0, color);
        self.text(
            label,
            Rect::new(start + icon_w + gap, rect.t, rect.r, rect.b),
            size,
            weight,
            color,
            HAlign::Left,
            true,
        );
    }

    /// Текст в прямоугольнике: halign + вертикальное центрирование по желанию.
    pub fn text(
        &self,
        s: &str,
        rect: Rect,
        size: f32,
        weight: u32,
        color: D2D1_COLOR_F,
        halign: HAlign,
        vcenter: bool,
    ) {
        let Some(rt) = &self.rt else { return };
        let wide: Vec<u16> = s.encode_utf16().collect();
        let format = self.format(size, weight, false);
        unsafe {
            let _ = format.SetTextAlignment(match halign {
                HAlign::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                HAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                HAlign::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            let _ = format.SetParagraphAlignment(if vcenter {
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER
            } else {
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR
            });
            rt.DrawText(
                &wide,
                &format,
                &rect.d2d(),
                &self.brush(color),
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// Ширина строки без переносов (для позиционирования соседних надписей).
    pub fn measure_width(&self, s: &str, size: f32, weight: u32) -> f32 {
        let Ok(layout) = self.layout(s, size, weight, f32::MAX, false) else {
            return 0.0;
        };
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = layout.GetMetrics(&mut m);
        }
        m.widthIncludingTrailingWhitespace
    }

    pub fn layout(
        &self,
        s: &str,
        size: f32,
        weight: u32,
        max_w: f32,
        wrap: bool,
    ) -> windows::core::Result<IDWriteTextLayout> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let format = self.format(size, weight, wrap);
        unsafe {
            let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            self.dwrite.CreateTextLayout(&wide, &format, max_w, f32::MAX)
        }
    }

    pub fn layout_height(layout: &IDWriteTextLayout) -> f32 {
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = layout.GetMetrics(&mut m);
        }
        m.height
    }

    pub fn draw_layout(&self, layout: &IDWriteTextLayout, x: f32, y: f32, color: D2D1_COLOR_F) {
        if let Some(rt) = &self.rt {
            unsafe {
                rt.DrawTextLayout(
                    D2D_POINT_2F { x, y },
                    layout,
                    &self.brush(color),
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                );
            }
        }
    }
}

/// Шрифт-иконки: на Windows 11 это Segoe Fluent Icons, на 10 — запасной
/// Segoe MDL2 Assets (общие глифы делят одни и те же коды).
fn detect_icon_family(dwrite: &IDWriteFactory) -> Vec<u16> {
    let fluent: Vec<u16> = "Segoe Fluent Icons\0".encode_utf16().collect();
    unsafe {
        let mut coll: Option<IDWriteFontCollection> = None;
        if dwrite.GetSystemFontCollection(&mut coll, BOOL(0)).is_ok() {
            if let Some(coll) = coll {
                let mut index = 0u32;
                let mut exists = BOOL(0);
                if coll
                    .FindFamilyName(PCWSTR(fluent.as_ptr()), &mut index, &mut exists)
                    .is_ok()
                    && exists.as_bool()
                {
                    return fluent;
                }
            }
        }
    }
    "Segoe MDL2 Assets\0".encode_utf16().collect()
}
