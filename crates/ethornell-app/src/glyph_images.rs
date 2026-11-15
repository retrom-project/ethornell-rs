//! Owned bitmap glyphs. Native character codes share a 256-entry table; Unicode
//! text addresses it through U+F001..U+F7FF, independently of the font size.
use super::{DecodedImage, RuntimeTraceApi};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

const MAX_BYTES: usize = 128 * 1024 * 1024;
type SharedImages = Arc<RwLock<BTreeMap<u8, Option<Arc<DecodedImage>>>>>;

thread_local! {
    static SESSION: RefCell<Option<SharedImages>> = const { RefCell::new(None) };
}

#[derive(Clone, Default)]
pub(crate) struct GlyphImages {
    images: SharedImages,
}

pub(crate) struct GlyphSessionGuard {
    previous: Option<SharedImages>,
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl Drop for GlyphSessionGuard {
    fn drop(&mut self) {
        SESSION.with(|session| *session.borrow_mut() = self.previous.take());
    }
}

fn slot(code: i32) -> Option<u8> {
    let value = code as u32;
    let valid = if code < 0 {
        (value & 0x7fff_ffff).wrapping_sub(0xf001) <= 0x7fe
    } else {
        value.wrapping_sub(0xff01) <= 0xfe
    };
    valid.then_some(value as u8)
}

fn text_slot(ch: char) -> Option<u8> {
    (0xf001..=0xf7ff).contains(&(ch as u32)).then_some(ch as u8)
}

pub(crate) fn current(ch: char) -> Option<Arc<DecodedImage>> {
    let slot = text_slot(ch)?;
    SESSION.with(|session| {
        let images = session.borrow();
        let images = images
            .as_ref()?
            .read()
            .unwrap_or_else(|error| error.into_inner());
        images.get(&slot).cloned().flatten()
    })
}

pub(crate) fn handled(ch: char) -> bool {
    let Some(slot) = text_slot(ch) else {
        return false;
    };
    SESSION.with(|session| {
        session.borrow().as_ref().is_some_and(|images| {
            images
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .contains_key(&slot)
        })
    })
}

impl GlyphImages {
    pub(crate) fn enter_session(&self) -> GlyphSessionGuard {
        GlyphSessionGuard {
            previous: SESSION.with(|session| session.replace(Some(self.images.clone()))),
            _not_send: std::marker::PhantomData,
        }
    }

    pub(crate) fn contains_text(&self, text: &str) -> bool {
        let images = self
            .images
            .read()
            .unwrap_or_else(|error| error.into_inner());
        text.chars()
            .filter_map(text_slot)
            .any(|slot| images.contains_key(&slot))
    }

    fn clear(&self, slot: u8) {
        self.images
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .insert(slot, None);
    }

    fn set_crop(
        &self,
        slot: u8,
        source: &DecodedImage,
        crop: [i32; 4],
        xrgb: bool,
    ) -> Result<(), String> {
        let [x, y, width, height] = crop;
        let length = usize::try_from(width).ok().and_then(|width| {
            usize::try_from(height)
                .ok()?
                .checked_mul(width)?
                .checked_mul(4)
        });
        let Some(length) = length.filter(|&length| length > 0 && length <= MAX_BYTES) else {
            return Err("glyph image allocation budget exceeded".into());
        };
        if width > 16_384
            || height > 16_384
            || u64::from(source.width) * u64::from(source.height) * 4 != source.rgba.len() as u64
        {
            return Err("invalid glyph RGBA image dimensions".into());
        }
        let mut images = self
            .images
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let retained = images
            .iter()
            .filter(|(index, _)| **index != slot)
            .filter_map(|(_, image)| image.as_ref().map(|image| image.rgba.len()))
            .sum::<usize>();
        if retained.saturating_add(length) > MAX_BYTES {
            return Err("glyph image catalog budget exceeded".into());
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(length)
            .map_err(|_| "glyph image allocation failed")?;
        rgba.resize(length, 0);
        if xrgb {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        for dy in 0..height {
            let sy = i64::from(y) + i64::from(dy);
            if !(0..i64::from(source.height)).contains(&sy) {
                continue;
            }
            for dx in 0..width {
                let sx = i64::from(x) + i64::from(dx);
                if !(0..i64::from(source.width)).contains(&sx) {
                    continue;
                }
                let src = ((sy as usize * source.width as usize) + sx as usize) * 4;
                let dst = ((dy as usize * width as usize) + dx as usize) * 4;
                rgba[dst..dst + 4].copy_from_slice(&source.rgba[src..src + 4]);
                if xrgb {
                    rgba[dst + 3] = 255;
                }
            }
        }
        images.insert(
            slot,
            Some(Arc::new(DecodedImage {
                width: width as u32,
                height: height as u32,
                rgba,
            })),
        );
        Ok(())
    }
}

impl RuntimeTraceApi {
    pub(super) fn configure_glyph_image(
        &mut self,
        raw_code: i32,
        source: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> ethornell_vm::VmResult<()> {
        let Some(slot) = slot(raw_code) else {
            return Err(ethornell_vm::VmError::Runtime(format!(
                "無効な文字コード [ {raw_code} ] が指定されました"
            )));
        };
        if source == -1 {
            self.glyph_images.clear(slot);
            return Ok(());
        }
        let image = self.graph_bitmap_image(source).ok_or_else(|| {
            ethornell_vm::VmError::Runtime(format!(
                "無効なビットマップ [ {source} ] が指定されました"
            ))
        })?;
        if width == 0 || height == 0 {
            return Err(ethornell_vm::VmError::Runtime(format!(
                "無効なイメージサイズ [ {width} , {height} ] が指定されました"
            )));
        }
        self.glyph_images
            .set_crop(
                slot,
                &image,
                [x, y, width, height],
                self.bitmap_formats.get(&source) == Some(&1),
            )
            .map_err(ethornell_vm::VmError::Runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> DecodedImage {
        DecodedImage {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 9, 8, 7, 0],
        }
    }

    #[test]
    fn native_ranges_and_shared_low_byte_are_preserved() {
        for (code, expected) in [
            (0xff01, Some(1)),
            (0xffff, Some(255)),
            (0xff00, None),
            (0xf001, None),
            (0x8000_f001_u32 as i32, Some(1)),
            (0x8000_f100_u32 as i32, Some(0)),
            (0x8000_f7ff_u32 as i32, Some(255)),
            (0x8000_f000_u32 as i32, None),
            (0x8000_f800_u32 as i32, None),
        ] {
            assert_eq!(slot(code), expected);
        }
    }

    #[test]
    fn crop_owns_original_rgba_pixels_after_source_mutation_and_drop() {
        let images = GlyphImages::default();
        let mut src = source();
        images.set_crop(1, &src, [1, 0, 1, 2], false).unwrap();
        src.rgba.fill(99);
        drop(src);
        let _session = images.enter_session();
        let glyph = current('\u{f001}').unwrap();
        assert_eq!((glyph.width, glyph.height), (1, 2));
        assert_eq!(glyph.rgba, [0, 255, 0, 128, 9, 8, 7, 0]);
    }

    #[test]
    fn out_of_bounds_crop_is_zero_filled_and_xrgb_ignores_source_alpha() {
        let images = GlyphImages::default();
        images
            .set_crop(1, &source(), [-1, -1, 2, 2], false)
            .unwrap();
        let _session = images.enter_session();
        assert_eq!(
            current('\u{f001}').unwrap().rgba,
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 255]
        );
        images.set_crop(1, &source(), [1, 0, 1, 2], true).unwrap();
        assert_eq!(
            current('\u{f001}').unwrap().rgba,
            [0, 255, 0, 255, 9, 8, 7, 255]
        );
    }

    #[test]
    fn clearing_and_alias_updates_are_visible_in_an_existing_session() {
        let images = GlyphImages::default();
        let _session = images.enter_session();
        images
            .set_crop(slot(0xff01).unwrap(), &source(), [0, 0, 1, 1], false)
            .unwrap();
        assert_eq!(current('\u{f101}').unwrap().rgba, [255, 0, 0, 255]);
        images.clear(slot(0x8000_f101_u32 as i32).unwrap());
        assert!(current('\u{f001}').is_none());
        assert!(images.contains_text("a\u{f001}"));
        assert!(current('\u{ff01}').is_none());
    }

    #[test]
    fn scoped_catalogs_restore_and_failed_replacement_preserves_pixels() {
        let a = GlyphImages::default();
        let b = GlyphImages::default();
        a.set_crop(1, &source(), [0, 0, 1, 1], false).unwrap();
        let _outer = a.enter_session();
        {
            let _inner = b.enter_session();
            assert!(current('\u{f001}').is_none());
        }
        assert!(
            a.set_crop(1, &source(), [0, 0, i32::MAX, i32::MAX], false)
                .is_err()
        );
        assert!(a.set_crop(1, &source(), [0, 0, 0, 1], false).is_err());
        assert_eq!(current('\u{f001}').unwrap().rgba, [255, 0, 0, 255]);
    }

    #[test]
    fn cpu_text_draws_original_colours_at_natural_size_and_uses_image_advance() {
        let font = crate::snapshot::load_default_font().expect("test requires session font");
        let _font = crate::snapshot::enter_font_session(&font);
        let images = GlyphImages::default();
        let mut src = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
        };
        images.set_crop(1, &src, [0, 0, 2, 1], false).unwrap();
        src.rgba.clear();
        drop(src);
        let _session = images.enter_session();
        assert_eq!(crate::snapshot::glyph_advance('\u{f001}', 12.0, 100.0), 2.0);
        assert_eq!(
            crate::snapshot::glyph_advance('\u{f001}', 100.0, 100.0),
            2.0
        );
        assert_eq!(
            crate::snapshot::measure_text_advance("\u{f001}", 100.0, 0.0),
            2
        );
        let mut out = DecodedImage {
            width: 5,
            height: 3,
            rgba: vec![0; 5 * 3 * 4],
        };
        crate::snapshot::rasterize_placed_glyph_records(
            &mut out,
            2,
            &[crate::text_layout::PlacedGlyph {
                ch: '\u{f001}',
                index: 0,
                x: 1.0,
                y: 1.0,
            }],
            100.0,
            [0.0, 0.0, 1.0, 1.0],
            &|_| None,
            None,
        );
        assert_eq!(
            &out.rgba[(5 + 1) * 4..(5 + 3) * 4],
            &[255, 0, 0, 255, 0, 255, 0, 255]
        );
        assert_eq!(
            out.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] != 0)
                .count(),
            2
        );
        assert!(!crate::snapshot::take_missing_glyphs());
        images.clear(1);
        assert!(images.contains_text("\u{f001}"));
        assert_eq!(
            crate::snapshot::glyph_advance('\u{f001}', 100.0, 100.0),
            0.0
        );
        out.rgba.fill(0);
        crate::snapshot::rasterize_placed_glyph_records(
            &mut out,
            2,
            &[crate::text_layout::PlacedGlyph {
                ch: '\u{f001}',
                index: 0,
                x: 1.0,
                y: 1.0,
            }],
            100.0,
            [1.0; 4],
            &|_| None,
            None,
        );
        assert!(out.rgba.iter().all(|&byte| byte == 0));
        assert!(!crate::snapshot::take_missing_glyphs());
    }
}
