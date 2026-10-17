//! Original Raumakey icon artwork, rendered from code, and its ICO/DIB encoding.
//!
//! Every icon is a rounded square with a white letter:
//!
//! - app mark: "R" with an amber acute accent on teal (`#0E7C66`),
//! - Vietnamese mode: "V" on teal,
//! - English mode: "E" on slate (`#475569`).
//!
//! Shapes are described in pixel coordinates for the requested size; stems and bars are snapped
//! to whole pixels so small sizes stay crisp, and edges are anti-aliased with 8 × 8 samples per
//! pixel. Pixels are stored as straight (not premultiplied) BGRA, as icons expect.

use crate::error::ResourceError;

/// Sizes rendered for every icon group: 100 %, 125 %, 150 %, 200 %, and 300 % of 16 px.
pub const ICON_SIZES: [u32; 5] = [16, 20, 24, 32, 48];

/// Largest icon size the ICO and group-icon directories can describe.
pub const MAX_ICON_SIZE: u32 = 256;

/// Anti-aliasing samples per pixel axis.
const SAMPLES: u32 = 8;

const TEAL: [u8; 3] = [0x0E, 0x7C, 0x66];
const SLATE: [u8; 3] = [0x47, 0x55, 0x69];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const AMBER: [u8; 3] = [0xFB, 0xBF, 0x24];

/// Extra horizontal width (pixels) given to diagonal strokes so they look as heavy as the
/// pixel-snapped vertical stems after anti-aliasing.
const DIAGONAL_COMPENSATION: f32 = 0.5;

/// Size of a `BITMAPINFOHEADER`.
const BITMAP_INFO_HEADER_SIZE: usize = 40;

/// A Raumakey icon design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconGlyph {
    /// Application mark: white "R" with an amber acute accent on a teal rounded square.
    AppMark,
    /// Vietnamese input mode: white "V" on a teal rounded square.
    Vietnamese,
    /// English (pass-through) mode: white "E" on a slate rounded square.
    English,
}

/// A rendered square icon image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconImage {
    size: u32,
    /// Straight-alpha BGRA pixels, top row first.
    bgra: Vec<u8>,
}

impl IconImage {
    /// Renders `glyph` at `size` × `size` pixels; `size` must be in `1..=256`.
    pub fn render(glyph: IconGlyph, size: u32) -> Result<Self, ResourceError> {
        if !(1..=MAX_ICON_SIZE).contains(&size) {
            return Err(ResourceError::InvalidIconSize);
        }
        let scene = scene(glyph, size);
        Ok(Self {
            size,
            bgra: rasterize(&scene, size),
        })
    }

    /// Width and height in pixels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Straight-alpha BGRA pixels, top row first.
    pub fn bgra(&self) -> &[u8] {
        &self.bgra
    }

    /// Returns the `[blue, green, red, alpha]` pixel at column `x`, row `y` (top row 0).
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size || y >= self.size {
            return None;
        }
        let index = (y as usize * self.size as usize + x as usize) * 4;
        self.bgra
            .get(index..index + 4)
            .and_then(|pixel| pixel.try_into().ok())
    }

    /// Encodes the image as stored in an `.ico` file or an `RT_ICON` resource: a
    /// `BITMAPINFOHEADER` with doubled height, bottom-up 32-bit BGRA rows, then a bottom-up 1-bit
    /// AND mask (rows padded to 32 bits) that is set where the pixel is fully transparent.
    pub fn to_dib(&self) -> Vec<u8> {
        // `size` is at most 256, so none of these products can overflow.
        let width = self.size as usize;
        let row_bytes = width * 4;
        let mask_stride = width.div_ceil(32) * 4;
        let xor_len = row_bytes * width;
        let and_len = mask_stride * width;
        let mut out = Vec::with_capacity(BITMAP_INFO_HEADER_SIZE + xor_len + and_len);
        out.extend_from_slice(&(BITMAP_INFO_HEADER_SIZE as u32).to_le_bytes()); // biSize
        out.extend_from_slice(&(self.size as i32).to_le_bytes()); // biWidth
        out.extend_from_slice(&(2 * self.size as i32).to_le_bytes()); // biHeight
        out.extend_from_slice(&1_u16.to_le_bytes()); // biPlanes
        out.extend_from_slice(&32_u16.to_le_bytes()); // biBitCount
        out.extend_from_slice(&0_u32.to_le_bytes()); // biCompression = BI_RGB
        out.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes()); // biSizeImage
        out.extend_from_slice(&[0; 16]); // resolution and palette fields
        for row in self.bgra.chunks_exact(row_bytes).rev() {
            out.extend_from_slice(row);
        }
        let mut mask_row = vec![0_u8; mask_stride];
        for row in self.bgra.chunks_exact(row_bytes).rev() {
            mask_row.fill(0);
            for (x, pixel) in row.as_chunks::<4>().0.iter().enumerate() {
                if let [_, _, _, 0] = *pixel
                    && let Some(byte) = mask_row.get_mut(x / 8)
                {
                    *byte |= 0x80 >> (x % 8);
                }
            }
            out.extend_from_slice(&mask_row);
        }
        out
    }
}

/// Renders `glyph` at every size in [`ICON_SIZES`], smallest first.
pub fn render_icon_set(glyph: IconGlyph) -> Result<Vec<IconImage>, ResourceError> {
    ICON_SIZES
        .iter()
        .map(|&size| IconImage::render(glyph, size))
        .collect()
}

/// Width or height as stored in an icon directory entry (0 means 256).
pub(crate) fn dimension_byte(size: u32) -> u8 {
    u8::try_from(size).unwrap_or(0)
}

/// Builds a standalone `.ico` file: an `ICONDIR`, one `ICONDIRENTRY` per image with the byte
/// offset of its data, then the images in order.
pub fn ico_file(images: &[IconImage]) -> Result<Vec<u8>, ResourceError> {
    if images.is_empty() {
        return Err(ResourceError::EmptyIconGroup);
    }
    let count = u16::try_from(images.len()).map_err(|_| ResourceError::TooLarge)?;
    let dibs: Vec<Vec<u8>> = images.iter().map(IconImage::to_dib).collect();
    let directory_len = 6 + 16 * images.len();
    let total = dibs
        .iter()
        .try_fold(directory_len, |total, dib| total.checked_add(dib.len()))
        .ok_or(ResourceError::TooLarge)?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0_u16.to_le_bytes()); // idReserved
    out.extend_from_slice(&1_u16.to_le_bytes()); // idType = icon
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = directory_len;
    for (image, dib) in images.iter().zip(&dibs) {
        let length = u32::try_from(dib.len()).map_err(|_| ResourceError::TooLarge)?;
        let image_offset = u32::try_from(offset).map_err(|_| ResourceError::TooLarge)?;
        out.push(dimension_byte(image.size()));
        out.push(dimension_byte(image.size()));
        out.push(0); // bColorCount
        out.push(0); // bReserved
        out.extend_from_slice(&1_u16.to_le_bytes()); // wPlanes
        out.extend_from_slice(&32_u16.to_le_bytes()); // wBitCount
        out.extend_from_slice(&length.to_le_bytes()); // dwBytesInRes
        out.extend_from_slice(&image_offset.to_le_bytes()); // dwImageOffset
        offset += dib.len();
    }
    for dib in &dibs {
        out.extend_from_slice(dib);
    }
    Ok(out)
}

/// A filled shape in pixel coordinates (x right, y down).
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Axis-aligned rectangle `[x0, x1) × [y0, y1)`.
    Rect { x0: f32, y0: f32, x1: f32, y1: f32 },
    /// Axis-aligned rectangle with circular corners.
    RoundedRect {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        radius: f32,
    },
    /// Convex quadrilateral; vertices in order (either winding).
    Quad([(f32, f32); 4]),
    /// Right half (`x >= cx`) of a ring.
    HalfRing {
        cx: f32,
        cy: f32,
        outer: f32,
        inner: f32,
    },
}

impl Shape {
    /// Bounding box `(x0, y0, x1, y1)`.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        match *self {
            Self::Rect { x0, y0, x1, y1 } | Self::RoundedRect { x0, y0, x1, y1, .. } => {
                (x0, y0, x1, y1)
            }
            Self::Quad(points) => points.iter().fold(
                (
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ),
                |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            ),
            Self::HalfRing { cx, cy, outer, .. } => (cx, cy - outer, cx + outer, cy + outer),
        }
    }

    /// Whether the bounding box intersects the open pixel square at `(x, y)`.
    fn may_touch(&self, x: f32, y: f32) -> bool {
        let (x0, y0, x1, y1) = self.bounds();
        x0 < x + 1.0 && x1 > x && y0 < y + 1.0 && y1 > y
    }

    /// Whether every sample of the pixel square at `(x, y)` lies inside the shape. Only convex
    /// shapes answer `true`: a convex shape that contains the four corners contains the square.
    fn covers(&self, x: f32, y: f32) -> bool {
        match *self {
            Self::Rect { x0, y0, x1, y1 } => x >= x0 && x + 1.0 <= x1 && y >= y0 && y + 1.0 <= y1,
            Self::RoundedRect {
                x0,
                y0,
                x1,
                y1,
                radius,
            } => [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)]
                .into_iter()
                .all(|(cx, cy)| {
                    let inside = cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1;
                    let nearest_x = cx.clamp(x0 + radius, x1 - radius);
                    let nearest_y = cy.clamp(y0 + radius, y1 - radius);
                    let (dx, dy) = (cx - nearest_x, cy - nearest_y);
                    inside && dx * dx + dy * dy <= radius * radius
                }),
            Self::Quad(_) => [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)]
                .into_iter()
                .all(|(cx, cy)| self.contains(cx, cy)),
            Self::HalfRing { .. } => false,
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        match *self {
            Self::Rect { x0, y0, x1, y1 } => x >= x0 && x < x1 && y >= y0 && y < y1,
            Self::RoundedRect {
                x0,
                y0,
                x1,
                y1,
                radius,
            } => {
                if x < x0 || x >= x1 || y < y0 || y >= y1 {
                    return false;
                }
                let nearest_x = x.clamp(x0 + radius, x1 - radius);
                let nearest_y = y.clamp(y0 + radius, y1 - radius);
                let (dx, dy) = (x - nearest_x, y - nearest_y);
                dx * dx + dy * dy <= radius * radius
            }
            Self::Quad(points) => {
                let mut positive = false;
                let mut negative = false;
                let next = points.iter().cycle().skip(1);
                for (&(ax, ay), &(bx, by)) in points.iter().zip(next) {
                    let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
                    positive |= cross > 0.0;
                    negative |= cross < 0.0;
                }
                !(positive && negative)
            }
            Self::HalfRing {
                cx,
                cy,
                outer,
                inner,
            } => {
                let (dx, dy) = (x - cx, y - cy);
                let distance = dx * dx + dy * dy;
                x >= cx && distance <= outer * outer && distance >= inner * inner
            }
        }
    }
}

/// Shapes filled with one color; a scene is a list of layers, back to front.
struct Layer {
    color: [u8; 3],
    shapes: Vec<Shape>,
}

/// Rasterizes a scene into straight-alpha BGRA pixels, top row first.
///
/// Each sample takes the color of the front-most layer containing it. A pixel that a convex
/// shape covers completely, with nothing in front of it, is filled without sampling; samples
/// are only tested against shapes whose bounding boxes touch the pixel.
fn rasterize(scene: &[Layer], size: u32) -> Vec<u8> {
    let width = size as usize;
    let mut bgra = vec![0_u8; width * width * 4];
    let step = 1.0 / SAMPLES as f32;
    let total = SAMPLES * SAMPLES;
    let shapes: Vec<(usize, &Shape)> = scene
        .iter()
        .enumerate()
        .flat_map(|(layer, entry)| entry.shapes.iter().map(move |shape| (layer, shape)))
        .collect();
    let color = |layer: usize| scene.get(layer).map(|entry| entry.color);
    let mut front: Vec<(usize, &Shape)> = Vec::with_capacity(shapes.len());
    for (index, pixel) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (column, row) = ((index % width) as f32, (index / width) as f32);
        let touching = shapes
            .iter()
            .filter(|(_, shape)| shape.may_touch(column, row));
        // The front-most layer that covers the whole pixel; samples default to it.
        let base = touching
            .clone()
            .filter(|(_, shape)| shape.covers(column, row))
            .map(|&(layer, _)| layer)
            .max();
        front.clear();
        front.extend(touching.filter(|&&(layer, _)| base.is_none_or(|base| layer > base)));
        if front.is_empty() {
            if let Some([r, g, b]) = base.and_then(color) {
                *pixel = [b, g, r, 0xFF];
            }
            continue;
        }
        let mut sum = [0_u32; 3];
        let mut covered = 0_u32;
        for sample_y in 0..SAMPLES {
            let y = row + (sample_y as f32 + 0.5) * step;
            for sample_x in 0..SAMPLES {
                let x = column + (sample_x as f32 + 0.5) * step;
                let top = front
                    .iter()
                    .filter(|(_, shape)| shape.contains(x, y))
                    .map(|&(layer, _)| layer)
                    .max()
                    .or(base);
                if let Some(rgb) = top.and_then(color) {
                    for (channel, value) in sum.iter_mut().zip(rgb) {
                        *channel += u32::from(value);
                    }
                    covered += 1;
                }
            }
        }
        if covered > 0 {
            let average = |channel: u32| ((channel + covered / 2) / covered) as u8;
            let alpha = ((covered * 255 + total / 2) / total) as u8;
            let [r, g, b] = sum;
            *pixel = [average(b), average(g), average(r), alpha];
        }
    }
    bgra
}

/// Integer metrics shared by every glyph at one size.
struct Metrics {
    size: f32,
    /// Stroke width in whole pixels.
    stroke: f32,
    /// Letter top and bottom (cap height box), whole pixels.
    top: f32,
    bottom: f32,
}

impl Metrics {
    fn new(size: u32) -> Self {
        let stroke = ((size + 2) / 8).max(1);
        let margin = (size * 3 + 8) / 16;
        let height = size.saturating_sub(2 * margin).max(1);
        let top = size.saturating_sub(height) / 2;
        Self {
            size: size as f32,
            stroke: stroke as f32,
            top: top as f32,
            bottom: (top + height) as f32,
        }
    }

    fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// The whole-pixel width closest to `ideal` whose parity matches the icon size, so that a
    /// box of this width centers exactly.
    fn centered_width(&self, ideal: f32) -> f32 {
        let parity = self.size % 2.0;
        let half = ((ideal - parity) / 2.0).round().max(0.0);
        (2.0 * half + parity).clamp(1.0, self.size)
    }
}

fn scene(glyph: IconGlyph, size: u32) -> Vec<Layer> {
    let metrics = Metrics::new(size);
    let background = |color| Layer {
        color,
        shapes: vec![Shape::RoundedRect {
            x0: 0.0,
            y0: 0.0,
            x1: metrics.size,
            y1: metrics.size,
            radius: metrics.size * 3.0 / 16.0,
        }],
    };
    match glyph {
        IconGlyph::AppMark => {
            let (letter, accent) = letter_r_with_accent(&metrics);
            vec![
                background(TEAL),
                Layer {
                    color: WHITE,
                    shapes: letter,
                },
                Layer {
                    color: AMBER,
                    shapes: accent,
                },
            ]
        }
        IconGlyph::Vietnamese => vec![
            background(TEAL),
            Layer {
                color: WHITE,
                shapes: letter_v(&metrics),
            },
        ],
        IconGlyph::English => vec![
            background(SLATE),
            Layer {
                color: WHITE,
                shapes: letter_e(&metrics),
            },
        ],
    }
}

/// "E": a stem and three arms, all on whole pixels.
fn letter_e(m: &Metrics) -> Vec<Shape> {
    let width = m.centered_width(m.height() * 0.62).max(m.stroke + 1.0);
    let x0 = ((m.size - width) / 2.0).floor();
    let x1 = x0 + width;
    let middle_width = (width * 0.86).round().max(m.stroke);
    let middle_top = m.top + ((m.height() - m.stroke) / 2.0).floor();
    vec![
        Shape::Rect {
            x0,
            y0: m.top,
            x1: x0 + m.stroke,
            y1: m.bottom,
        },
        Shape::Rect {
            x0,
            y0: m.top,
            x1,
            y1: m.top + m.stroke,
        },
        Shape::Rect {
            x0,
            y0: middle_top,
            x1: x0 + middle_width,
            y1: middle_top + m.stroke,
        },
        Shape::Rect {
            x0,
            y0: m.bottom - m.stroke,
            x1,
            y1: m.bottom,
        },
    ]
}

/// "V": two diagonal strokes of uniform thickness meeting in a flat foot at the center.
fn letter_v(m: &Metrics) -> Vec<Shape> {
    let width = m.centered_width(m.height() * 0.8);
    let x0 = ((m.size - width) / 2.0).floor();
    let x1 = x0 + width;
    let center = m.size / 2.0;
    let height = m.height();
    let run = (width - m.stroke) / 2.0;
    // Horizontal thickness that gives the diagonal a perpendicular thickness of one stroke, plus
    // the weight that anti-aliasing takes away from diagonals compared with snapped stems.
    let slanted = m.stroke * (run * run + height * height).sqrt() / height;
    let thickness = (slanted + DIAGONAL_COMPENSATION).min(width / 2.0);
    let foot_left = center - thickness / 2.0;
    let foot_right = center + thickness / 2.0;
    vec![
        Shape::Quad([
            (x0, m.top),
            (x0 + thickness, m.top),
            (foot_right, m.bottom),
            (foot_left, m.bottom),
        ]),
        Shape::Quad([
            (x1 - thickness, m.top),
            (x1, m.top),
            (foot_right, m.bottom),
            (foot_left, m.bottom),
        ]),
    ]
}

/// "R" (stem, bowl, diagonal leg) followed by an acute accent at its upper right.
fn letter_r_with_accent(m: &Metrics) -> (Vec<Shape>, Vec<Shape>) {
    let height = m.height();
    let letter_width = (height * 0.66).round().max(m.stroke + 1.0);
    let gap = (m.stroke * 0.5).round().max(1.0);
    let accent_width = (height * 0.36).round().max(m.stroke + 1.0);
    let group = letter_width + gap + accent_width;
    let x0 = ((m.size - group) / 2.0).floor();
    let x1 = x0 + letter_width;

    let bowl_height = (height * 0.56).round().max(2.0 * m.stroke);
    let bowl_bottom = m.top + bowl_height;
    let outer = bowl_height / 2.0;
    let inner = (outer - m.stroke).max(0.0);
    let bowl_center_x = x1 - outer;
    let bowl_center_y = m.top + outer;
    let leg = m.stroke * 1.15 + DIAGONAL_COMPENSATION;
    let leg_top = bowl_bottom - m.stroke;
    let leg_left = bowl_center_x - leg / 2.0;
    let letter = vec![
        Shape::Rect {
            x0,
            y0: m.top,
            x1: x0 + m.stroke,
            y1: m.bottom,
        },
        Shape::Rect {
            x0,
            y0: m.top,
            x1: bowl_center_x,
            y1: m.top + m.stroke,
        },
        Shape::Rect {
            x0,
            y0: bowl_bottom - m.stroke,
            x1: bowl_center_x,
            y1: bowl_bottom,
        },
        Shape::HalfRing {
            cx: bowl_center_x,
            cy: bowl_center_y,
            outer,
            inner,
        },
        Shape::Quad([
            (leg_left, leg_top),
            (leg_left + leg, leg_top),
            (x1, m.bottom),
            (x1 - leg, m.bottom),
        ]),
    ];

    let accent_left = x1 + gap;
    let accent_right = accent_left + accent_width;
    let accent_top = (m.top - m.stroke / 2.0).floor().max(0.0);
    let accent_bottom = m.top + (height * 0.42).round();
    let thickness = m.stroke.max(1.5);
    let accent = vec![Shape::Quad([
        (accent_left, accent_bottom),
        (accent_left + thickness, accent_bottom),
        (accent_right, accent_top),
        (accent_right - thickness, accent_top),
    ])];
    (letter, accent)
}
