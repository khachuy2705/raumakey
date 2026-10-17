//! Icon artwork, DIB encoding, `.ico` files, and icon groups.

use crate::support::{find, i32_at, of_type, parse_res, u16_at, u32_at};
use raumakey_resources::{
    ICON_SIZES, IconGlyph, IconImage, LANG_EN_US, RT_GROUP_ICON, RT_ICON, ResWriter, ResourceError,
    ResourceId, ico_file, render_icon_set,
};

const GLYPHS: [IconGlyph; 3] = [
    IconGlyph::AppMark,
    IconGlyph::Vietnamese,
    IconGlyph::English,
];
const TEAL: [u8; 4] = [0x0E, 0x7C, 0x66, 0xFF];
const SLATE: [u8; 4] = [0x47, 0x55, 0x69, 0xFF];
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// Pixel as `[r, g, b, a]`.
fn rgba(image: &IconImage, x: u32, y: u32) -> [u8; 4] {
    let [b, g, r, a] = image.pixel(x, y).expect("pixel inside the image");
    [r, g, b, a]
}

fn white_mask(image: &IconImage) -> Vec<bool> {
    let size = image.size();
    (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .map(|(x, y)| rgba(image, x, y) == WHITE)
        .collect()
}

#[test]
fn standard_sizes_cover_100_to_300_percent_scaling() {
    assert_eq!(ICON_SIZES, [16, 20, 24, 32, 48]);
}

#[test]
fn renders_every_glyph_at_every_standard_size() {
    for glyph in GLYPHS {
        let set = render_icon_set(glyph).unwrap();
        let sizes: Vec<u32> = set.iter().map(IconImage::size).collect();
        assert_eq!(sizes, ICON_SIZES);
        for image in &set {
            let size = image.size() as usize;
            assert_eq!(image.bgra().len(), size * size * 4);
            assert_eq!(image.pixel(image.size(), 0), None);
            assert_eq!(image.pixel(0, image.size()), None);
        }
    }
}

#[test]
fn backgrounds_use_the_brand_colors() {
    for size in ICON_SIZES {
        let middle = size / 2;
        let app = IconImage::render(IconGlyph::AppMark, size).unwrap();
        let vietnamese = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        let english = IconImage::render(IconGlyph::English, size).unwrap();
        for x in [0, 1] {
            assert_eq!(rgba(&app, x, middle), TEAL, "app mark {size}px");
            assert_eq!(rgba(&vietnamese, x, middle), TEAL, "V {size}px");
            assert_eq!(rgba(&english, x, middle), SLATE, "E {size}px");
        }
    }
}

#[test]
fn rounded_corners_are_transparent() {
    for glyph in GLYPHS {
        for size in ICON_SIZES {
            let image = IconImage::render(glyph, size).unwrap();
            let last = size - 1;
            for (x, y) in [(0, 0), (last, 0), (0, last), (last, last)] {
                let alpha = rgba(&image, x, y)[3];
                assert!(
                    alpha <= 64,
                    "{glyph:?} {size}px corner ({x},{y}) alpha {alpha}"
                );
            }
        }
    }
}

#[test]
fn glyphs_are_drawn_in_white() {
    for glyph in GLYPHS {
        for size in ICON_SIZES {
            let image = IconImage::render(glyph, size).unwrap();
            let white = white_mask(&image).into_iter().filter(|&w| w).count();
            let total = (size * size) as usize;
            assert!(
                white * 100 >= total * 8,
                "{glyph:?} {size}px has only {white} white pixels"
            );
            assert!(
                white * 100 <= total * 45,
                "{glyph:?} {size}px is mostly white"
            );
        }
    }
}

#[test]
fn vietnamese_and_english_icons_differ() {
    for size in ICON_SIZES {
        let vietnamese = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        let english = IconImage::render(IconGlyph::English, size).unwrap();
        assert_ne!(vietnamese.bgra(), english.bgra());
        let v_mask = white_mask(&vietnamese);
        let e_mask = white_mask(&english);
        let differing = v_mask.iter().zip(&e_mask).filter(|(v, e)| v != e).count();
        let largest = v_mask
            .iter()
            .filter(|&&w| w)
            .count()
            .max(e_mask.iter().filter(|&&w| w).count());
        assert!(
            differing * 4 >= largest,
            "{size}px: V and E glyph shapes differ in only {differing} pixels"
        );
    }
}

#[test]
fn app_mark_differs_from_mode_glyphs_and_has_an_accent() {
    for size in ICON_SIZES {
        let app = IconImage::render(IconGlyph::AppMark, size).unwrap();
        let vietnamese = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        assert_ne!(white_mask(&app), white_mask(&vietnamese), "{size}px");
        let accent = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b, a] = rgba(&app, x, y);
                a == 0xFF && r >= 0xE0 && (0x90..=0xD0).contains(&g) && b <= 0x60
            })
            .count();
        assert!(accent >= 2, "{size}px app mark accent has {accent} pixels");
    }
}

#[test]
fn vietnamese_glyph_is_mirror_symmetric() {
    for size in ICON_SIZES {
        let image = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        for y in 0..size {
            for x in 0..size / 2 {
                let left = rgba(&image, x, y);
                let right = rgba(&image, size - 1 - x, y);
                for channel in 0..4 {
                    let delta = left[channel].abs_diff(right[channel]);
                    assert!(delta <= 8, "{size}px ({x},{y}) channel {channel}");
                }
            }
        }
    }
}

#[test]
fn rendering_is_deterministic() {
    for glyph in GLYPHS {
        assert_eq!(
            render_icon_set(glyph).unwrap(),
            render_icon_set(glyph).unwrap()
        );
    }
}

#[test]
fn render_accepts_exactly_one_to_256_pixels() {
    assert_eq!(
        IconImage::render(IconGlyph::AppMark, 0),
        Err(ResourceError::InvalidIconSize)
    );
    assert_eq!(
        IconImage::render(IconGlyph::English, 257),
        Err(ResourceError::InvalidIconSize)
    );
    for glyph in GLYPHS {
        for size in (1..=64).chain([255, 256]) {
            let image = IconImage::render(glyph, size).unwrap();
            assert_eq!(image.size(), size, "{glyph:?}");
        }
    }
}

#[test]
fn dib_is_32bpp_bottom_up_with_doubled_height_and_and_mask() {
    for size in ICON_SIZES.into_iter().chain([1, 33, 256]) {
        let image = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        let dib = image.to_dib();
        let width = size as usize;
        let mask_stride = width.div_ceil(32) * 4;
        let xor_len = width * width * 4;
        let and_len = mask_stride * width;
        assert_eq!(dib.len(), 40 + xor_len + and_len, "{size}px");
        assert_eq!(u32_at(&dib, 0), 40, "biSize");
        assert_eq!(i32_at(&dib, 4), size as i32, "biWidth");
        assert_eq!(
            i32_at(&dib, 8),
            2 * size as i32,
            "biHeight covers XOR and AND"
        );
        assert_eq!(u16_at(&dib, 12), 1, "biPlanes");
        assert_eq!(u16_at(&dib, 14), 32, "biBitCount");
        assert_eq!(u32_at(&dib, 16), 0, "biCompression = BI_RGB");
        assert_eq!(u32_at(&dib, 20) as usize, xor_len + and_len, "biSizeImage");
        assert!(dib[24..40].iter().all(|&byte| byte == 0));
        for y in 0..size {
            let row = (size - 1 - y) as usize;
            for x in 0..size {
                let pixel = image.pixel(x, y).unwrap();
                let offset = 40 + (row * width + x as usize) * 4;
                assert_eq!(dib[offset..offset + 4], pixel, "{size}px XOR ({x},{y})");
                let mask = dib[40 + xor_len + row * mask_stride + x as usize / 8];
                let transparent = (mask >> (7 - x % 8)) & 1 == 1;
                assert_eq!(transparent, pixel[3] == 0, "{size}px AND ({x},{y})");
            }
            for bit in width..mask_stride * 8 {
                let mask = dib[40 + xor_len + row * mask_stride + bit / 8];
                assert_eq!((mask >> (7 - bit % 8)) & 1, 0, "mask padding is zero");
            }
        }
    }
}

#[test]
fn ico_file_directory_offsets_are_consistent() {
    for glyph in GLYPHS {
        let images = render_icon_set(glyph).unwrap();
        let ico = ico_file(&images).unwrap();
        assert_eq!(u16_at(&ico, 0), 0, "idReserved");
        assert_eq!(u16_at(&ico, 2), 1, "idType = icon");
        assert_eq!(usize::from(u16_at(&ico, 4)), images.len(), "idCount");
        let mut expected_offset = 6 + 16 * images.len();
        for (index, image) in images.iter().enumerate() {
            let entry = 6 + 16 * index;
            assert_eq!(u32::from(ico[entry]), image.size(), "bWidth");
            assert_eq!(u32::from(ico[entry + 1]), image.size(), "bHeight");
            assert_eq!(ico[entry + 2], 0, "bColorCount");
            assert_eq!(ico[entry + 3], 0, "bReserved");
            assert_eq!(u16_at(&ico, entry + 4), 1, "wPlanes");
            assert_eq!(u16_at(&ico, entry + 6), 32, "wBitCount");
            let length = u32_at(&ico, entry + 8) as usize;
            let offset = u32_at(&ico, entry + 12) as usize;
            assert_eq!(offset, expected_offset, "images are contiguous");
            assert_eq!(ico[offset..offset + length], image.to_dib());
            expected_offset += length;
        }
        assert_eq!(ico.len(), expected_offset);
    }
}

#[test]
fn ico_file_rejects_empty_input_and_encodes_256_as_zero() {
    assert_eq!(ico_file(&[]), Err(ResourceError::EmptyIconGroup));
    let large = IconImage::render(IconGlyph::AppMark, 256).unwrap();
    let ico = ico_file(&[large]).unwrap();
    assert_eq!(ico[6], 0, "bWidth 0 means 256");
    assert_eq!(ico[7], 0, "bHeight 0 means 256");
}

#[test]
fn icon_groups_reference_their_rt_icon_entries() {
    let app = render_icon_set(IconGlyph::AppMark).unwrap();
    let vietnamese = render_icon_set(IconGlyph::Vietnamese).unwrap();
    let mut writer = ResWriter::new();
    writer.add_icon_group(1, &app).unwrap();
    writer.add_icon_group(2, &vietnamese).unwrap();
    let entries = parse_res(&writer.to_bytes());

    let icons = of_type(&entries, RT_ICON);
    assert_eq!(icons.len(), app.len() + vietnamese.len());
    let mut ids: Vec<u16> = icons
        .iter()
        .map(|entry| match entry.name {
            crate::support::Id::Ordinal(id) => id,
            crate::support::Id::Name(_) => panic!("RT_ICON names are ordinals"),
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), icons.len(), "RT_ICON ids are unique");
    assert_eq!(of_type(&entries, RT_GROUP_ICON).len(), 2);

    for (group_id, images) in [(1, &app), (2, &vietnamese)] {
        let group = find(&entries, RT_GROUP_ICON, group_id);
        assert_eq!(group.language, LANG_EN_US);
        assert_eq!(group.memory_flags, 0x1030, "MOVEABLE | PURE | DISCARDABLE");
        let data = &group.data;
        assert_eq!(u16_at(data, 0), 0, "idReserved");
        assert_eq!(u16_at(data, 2), 1, "idType = icon");
        assert_eq!(usize::from(u16_at(data, 4)), images.len(), "idCount");
        assert_eq!(data.len(), 6 + 14 * images.len());
        for (index, image) in images.iter().enumerate() {
            let entry = 6 + 14 * index;
            assert_eq!(u32::from(data[entry]), image.size(), "bWidth");
            assert_eq!(u32::from(data[entry + 1]), image.size(), "bHeight");
            assert_eq!(data[entry + 2], 0, "bColorCount");
            assert_eq!(data[entry + 3], 0, "bReserved");
            assert_eq!(u16_at(data, entry + 4), 1, "wPlanes");
            assert_eq!(u16_at(data, entry + 6), 32, "wBitCount");
            let length = u32_at(data, entry + 8) as usize;
            let icon_id = u16_at(data, entry + 12);
            let icon = find(&entries, RT_ICON, icon_id);
            assert_eq!(icon.data, image.to_dib(), "nID {icon_id} holds the image");
            assert_eq!(length, icon.data.len(), "dwBytesInRes");
            assert_eq!(icon.memory_flags, 0x1010, "MOVEABLE | DISCARDABLE");
            assert_eq!(icon.language, LANG_EN_US);
        }
    }
}

#[test]
fn failed_icon_groups_leave_the_writer_unchanged() {
    let images = render_icon_set(IconGlyph::English).unwrap();
    let mut writer = ResWriter::new();
    assert_eq!(
        writer.add_icon_group(1, &[]),
        Err(ResourceError::EmptyIconGroup)
    );
    assert_eq!(
        writer.add_icon_group(0, &images),
        Err(ResourceError::ZeroOrdinal)
    );
    assert!(writer.is_empty());
    writer.add_icon_group(1, &images).unwrap();
    let before = writer.to_bytes();
    assert_eq!(
        writer.add_icon_group(1, &images),
        Err(ResourceError::Duplicate)
    );
    assert_eq!(writer.to_bytes(), before);
    assert_eq!(writer.len(), images.len() + 1);
}

#[test]
fn icon_ids_skip_ordinals_already_in_use() {
    let images = render_icon_set(IconGlyph::Vietnamese).unwrap();
    let mut writer = ResWriter::new();
    let icon_type = ResourceId::ordinal(RT_ICON).unwrap();
    for id in [1, 3] {
        writer
            .add_raw(
                icon_type.clone(),
                ResourceId::ordinal(id).unwrap(),
                LANG_EN_US,
                0,
                images[0].to_dib(),
            )
            .unwrap();
    }
    writer.add_icon_group(7, &images).unwrap();
    let entries = parse_res(&writer.to_bytes());
    let group = find(&entries, RT_GROUP_ICON, 7);
    let ids: Vec<u16> = (0..images.len())
        .map(|index| u16_at(&group.data, 6 + 14 * index + 12))
        .collect();
    assert_eq!(ids, [2, 4, 5, 6, 7]);
}

/// Sizes between the exhaustively rendered `1..=64` and the 255/256 edge. Rendering every size
/// takes about half a minute in a debug build, so the range is sampled.
fn sampled_large_sizes() -> impl Iterator<Item = u32> {
    (65..=254).step_by(17).chain([254])
}

#[test]
fn sampled_large_sizes_keep_the_design() {
    for size in sampled_large_sizes() {
        let app = IconImage::render(IconGlyph::AppMark, size).unwrap();
        let vietnamese = IconImage::render(IconGlyph::Vietnamese, size).unwrap();
        let english = IconImage::render(IconGlyph::English, size).unwrap();
        let (middle, last) = (size / 2, size - 1);
        let total = (size * size) as usize;
        for (image, background) in [(&app, TEAL), (&vietnamese, TEAL), (&english, SLATE)] {
            assert_eq!(image.size(), size);
            assert_eq!(image.bgra().len(), total * 4, "{size}px");
            assert_eq!(rgba(image, 0, middle), background, "{size}px edge");
            for (x, y) in [(0, 0), (last, 0), (0, last), (last, last)] {
                assert_eq!(rgba(image, x, y)[3], 0, "{size}px corner ({x},{y})");
            }
            let white = white_mask(image).into_iter().filter(|&w| w).count();
            assert!(
                (total * 8..=total * 45).contains(&(white * 100)),
                "{size}px has {white} white pixels"
            );
        }
        assert_ne!(white_mask(&vietnamese), white_mask(&english), "{size}px");
        assert_ne!(white_mask(&app), white_mask(&vietnamese), "{size}px");
        let accent = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b, a] = rgba(&app, x, y);
                a == 0xFF && r >= 0xE0 && (0x90..=0xD0).contains(&g) && b <= 0x60
            })
            .count();
        assert!(accent * 100 >= total, "{size}px accent has {accent} pixels");
        for y in 0..size {
            for x in 0..size / 2 {
                let left = rgba(&vietnamese, x, y);
                let right = rgba(&vietnamese, last - x, y);
                for (l, r) in left.into_iter().zip(right) {
                    assert!(l.abs_diff(r) <= 8, "{size}px V asymmetric at ({x},{y})");
                }
            }
        }
    }
}
