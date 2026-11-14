use std::io::Cursor;

use exif::{Exif, In, Reader, Tag, Value};
use serde::Serialize;

const FUJI_SIGNATURE: &[u8] = b"FUJIFILM";
const MAX_FUJI_ENTRIES: usize = 1024;
const MAX_FACES: usize = 32;
const IFD_ENTRY_LEN: usize = 12;

const SATURATION: u16 = 0x1003;
const FOCUS_MODE: u16 = 0x1021;
const FOCUS_PIXEL: u16 = 0x1023;
const FOCUS_SETTINGS: u16 = 0x102d;
const GRAIN_ROUGHNESS: u16 = 0x1047;
const GRAIN_SIZE: u16 = 0x104c;
const BLUR_WARNING: u16 = 0x1300;
const FOCUS_WARNING: u16 = 0x1301;
const EXPOSURE_WARNING: u16 = 0x1302;
const FILM_MODE: u16 = 0x1401;
const DYNAMIC_RANGE_SETTING: u16 = 0x1402;
const DEVELOPMENT_DYNAMIC_RANGE: u16 = 0x1403;
const FACE_POSITIONS: u16 = 0x4103;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Area {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ShotInfo {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<u32>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    pub exposure_bias: Option<f64>,
    pub program: Option<String>,
    pub film_simulation: Option<String>,
    pub dynamic_range: Option<String>,
    pub grain: Option<String>,
    pub focus_mode: Option<String>,
    pub af_area: Option<String>,
    pub focus_point: Option<Point>,
    pub faces: Vec<Area>,
    pub focus_warning: Option<bool>,
    pub blur_warning: Option<bool>,
    pub exposure_warning: Option<bool>,
}

pub fn shot_info(jpeg_head: &[u8]) -> ShotInfo {
    let Ok(exif) = Reader::new().read_from_container(&mut Cursor::new(jpeg_head)) else {
        return ShotInfo::default();
    };
    let mut info = ShotInfo {
        camera: camera(&exif),
        lens: text(&exif, Tag::LensModel),
        focal_length: real(&exif, Tag::FocalLength).filter(|value| *value > 0.0),
        focal_length_35mm: whole(&exif, Tag::FocalLengthIn35mmFilm).filter(|value| *value > 0),
        exposure_time: exposure_time(&exif),
        f_number: real(&exif, Tag::FNumber).filter(|value| *value > 0.0),
        iso: whole(&exif, Tag::PhotographicSensitivity).filter(|value| *value > 0),
        exposure_bias: signed_real(&exif, Tag::ExposureBiasValue),
        program: whole(&exif, Tag::ExposureProgram).and_then(program),
        ..ShotInfo::default()
    };
    if let Some(notes) = maker_note(&exif).and_then(FujiNotes::parse) {
        let frame = Frame {
            width: whole(&exif, Tag::PixelXDimension),
            height: whole(&exif, Tag::PixelYDimension),
            orientation: whole(&exif, Tag::Orientation).unwrap_or(1),
        };
        notes.fill(&mut info, frame);
    }
    info
}

fn field(exif: &Exif, tag: Tag) -> Option<&Value> {
    exif.get_field(tag, In::PRIMARY).map(|field| &field.value)
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    match field(exif, tag)? {
        Value::Ascii(parts) => parts
            .first()
            .map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned())
            .filter(|value| !value.is_empty()),
        _ => None,
    }
}

fn whole(exif: &Exif, tag: Tag) -> Option<u32> {
    field(exif, tag)?.get_uint(0)
}

fn real(exif: &Exif, tag: Tag) -> Option<f64> {
    match field(exif, tag)? {
        Value::Rational(values) => values
            .first()
            .filter(|value| value.denom != 0)
            .map(|value| value.to_f64()),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn signed_real(exif: &Exif, tag: Tag) -> Option<f64> {
    match field(exif, tag)? {
        Value::SRational(values) => values
            .first()
            .filter(|value| value.denom != 0)
            .map(|value| value.to_f64()),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn camera(exif: &Exif) -> Option<String> {
    let model = text(exif, Tag::Model)?;
    let make = text(exif, Tag::Make).unwrap_or_default();
    let brand = make.split_whitespace().next().unwrap_or_default();
    if brand.is_empty() || model.to_lowercase().contains(&brand.to_lowercase()) {
        return Some(model);
    }
    let brand = match brand.to_uppercase().as_str() {
        "FUJIFILM" => "Fujifilm".to_owned(),
        _ => brand.to_owned(),
    };
    Some(format!("{brand} {model}"))
}

fn exposure_time(exif: &Exif) -> Option<String> {
    let Value::Rational(values) = field(exif, Tag::ExposureTime)? else {
        return None;
    };
    let value = values
        .first()
        .filter(|value| value.denom != 0 && value.num != 0)?;
    let seconds = value.to_f64();
    if seconds >= 0.3 {
        let rounded = (seconds * 10.0).round() / 10.0;
        return Some(if rounded.fract() == 0.0 {
            format!("{rounded:.0}")
        } else {
            format!("{rounded:.1}")
        });
    }
    Some(format!("1/{}", (1.0 / seconds).round()))
}

fn program(value: u32) -> Option<String> {
    let name = match value {
        1 => "Manual",
        2 => "Program",
        3 => "Aperture priority",
        4 => "Shutter priority",
        _ => return None,
    };
    Some(name.to_owned())
}

fn maker_note(exif: &Exif) -> Option<&[u8]> {
    match field(exif, Tag::MakerNote)? {
        Value::Undefined(bytes, _) => Some(bytes.as_slice()),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    width: Option<u32>,
    height: Option<u32>,
    orientation: u32,
}

impl Frame {
    fn normalize(&self, x: u16, y: u16) -> Option<Point> {
        let width = f64::from(self.width.filter(|value| *value > 0)?);
        let height = f64::from(self.height.filter(|value| *value > 0)?);
        let (x, y) = (f64::from(x) / width, f64::from(y) / height);
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        Some(oriented(Point { x, y }, self.orientation))
    }
}

pub fn oriented(point: Point, orientation: u32) -> Point {
    let Point { x, y } = point;
    let (x, y) = match orientation {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (1.0 - y, x),
        7 => (1.0 - y, 1.0 - x),
        8 => (y, 1.0 - x),
        _ => (x, y),
    };
    Point { x, y }
}

struct Entry<'a> {
    tag: u16,
    kind: u16,
    count: u32,
    data: &'a [u8],
}

struct FujiNotes<'a> {
    entries: Vec<Entry<'a>>,
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn type_size(kind: u16) -> Option<usize> {
    match kind {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 => Some(4),
        5 | 10 | 12 => Some(8),
        _ => None,
    }
}

impl<'a> FujiNotes<'a> {
    fn parse(note: &'a [u8]) -> Option<FujiNotes<'a>> {
        if !note.starts_with(FUJI_SIGNATURE) {
            return None;
        }
        let ifd = usize::try_from(le_u32(note, FUJI_SIGNATURE.len())?).ok()?;
        let count = usize::from(le_u16(note, ifd)?).min(MAX_FUJI_ENTRIES);
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let at = ifd
                .checked_add(2)?
                .checked_add(index.checked_mul(IFD_ENTRY_LEN)?)?;
            let Some(tag) = le_u16(note, at) else { break };
            let (Some(kind), Some(count)) = (le_u16(note, at + 2), le_u32(note, at + 4)) else {
                break;
            };
            let Some(length) =
                type_size(kind).and_then(|size| size.checked_mul(usize::try_from(count).ok()?))
            else {
                continue;
            };
            let start = if length <= 4 {
                at + 8
            } else {
                match le_u32(note, at + 8).and_then(|offset| usize::try_from(offset).ok()) {
                    Some(offset) => offset,
                    None => continue,
                }
            };
            let Some(data) = start
                .checked_add(length)
                .and_then(|end| note.get(start..end))
            else {
                continue;
            };
            entries.push(Entry {
                tag,
                kind,
                count,
                data,
            });
        }
        Some(FujiNotes { entries })
    }

    fn entry(&self, tag: u16) -> Option<&Entry<'a>> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }

    fn shorts(&self, tag: u16) -> Option<Vec<u16>> {
        let entry = self.entry(tag).filter(|entry| entry.kind == 3)?;
        let count = usize::try_from(entry.count).ok()?;
        (0..count)
            .map(|index| le_u16(entry.data, index * 2))
            .collect()
    }

    fn number(&self, tag: u16) -> Option<u32> {
        let entry = self.entry(tag)?;
        match entry.kind {
            3 | 8 => le_u16(entry.data, 0).map(u32::from),
            4 | 9 => le_u32(entry.data, 0),
            1 | 7 => entry.data.first().copied().map(u32::from),
            _ => None,
        }
    }

    fn flag(&self, tag: u16) -> Option<bool> {
        self.number(tag).map(|value| value != 0)
    }

    fn fill(&self, info: &mut ShotInfo, frame: Frame) {
        info.film_simulation = self.film_simulation();
        info.dynamic_range = self.dynamic_range();
        info.grain = self.grain();
        info.focus_mode = self.focus_mode();
        info.af_area = self.af_area();
        info.focus_point = self
            .shorts(FOCUS_PIXEL)
            .filter(|values| values.len() == 2)
            .and_then(|values| frame.normalize(values[0], values[1]));
        info.faces = self.faces(frame);
        info.focus_warning = self.flag(FOCUS_WARNING);
        info.blur_warning = self.flag(BLUR_WARNING);
        info.exposure_warning = self.flag(EXPOSURE_WARNING);
    }

    fn film_simulation(&self) -> Option<String> {
        let monochrome = match self.number(SATURATION) {
            Some(0x300) => Some("Monochrome"),
            Some(0x301) => Some("Monochrome + R"),
            Some(0x302) => Some("Monochrome + Ye"),
            Some(0x303) => Some("Monochrome + G"),
            Some(0x310) => Some("Sepia"),
            Some(0x500) => Some("Acros"),
            Some(0x501) => Some("Acros + R"),
            Some(0x502) => Some("Acros + Ye"),
            Some(0x503) => Some("Acros + G"),
            _ => None,
        };
        if let Some(name) = monochrome {
            return Some(name.to_owned());
        }
        let name = match self.number(FILM_MODE)? {
            0x000 => "Provia",
            0x100 | 0x110 | 0x130 | 0x300 => "Studio Portrait",
            0x120 => "Astia",
            0x200 | 0x400 => "Velvia",
            0x500 => "Pro Neg. Std",
            0x501 => "Pro Neg. Hi",
            0x600 => "Classic Chrome",
            0x700 => "Eterna",
            0x800 => "Classic Negative",
            0x900 => "Eterna Bleach Bypass",
            0xa00 => "Nostalgic Neg.",
            0xb00 => "Reala Ace",
            _ => return None,
        };
        Some(name.to_owned())
    }

    fn dynamic_range(&self) -> Option<String> {
        match self.number(DEVELOPMENT_DYNAMIC_RANGE) {
            Some(percent @ (100 | 200 | 400 | 800)) => Some(format!("DR{percent}")),
            _ => match self.number(DYNAMIC_RANGE_SETTING)? {
                0 => Some("DR Auto".to_owned()),
                0x100 => Some("DR100".to_owned()),
                _ => None,
            },
        }
    }

    fn grain(&self) -> Option<String> {
        let roughness = match self.number(GRAIN_ROUGHNESS)? {
            0 => return Some("Off".to_owned()),
            16 | 32 => "Weak",
            48 | 64 => "Strong",
            _ => return None,
        };
        Some(match self.number(GRAIN_SIZE) {
            Some(16) => format!("{roughness}, small"),
            Some(32) => format!("{roughness}, large"),
            _ => roughness.to_owned(),
        })
    }

    fn focus_mode(&self) -> Option<String> {
        if let Some(settings) = self.number(FOCUS_SETTINGS) {
            return match settings & 0x0f {
                0 => Some("MF".to_owned()),
                1 => Some("AF-S".to_owned()),
                2 => Some("AF-C".to_owned()),
                _ => None,
            };
        }
        match self.number(FOCUS_MODE)? {
            1 => Some("MF".to_owned()),
            0 => Some("AF".to_owned()),
            _ => None,
        }
    }

    fn af_area(&self) -> Option<String> {
        let settings = self.number(FOCUS_SETTINGS)?;
        if settings & 0x0f == 0 {
            return None;
        }
        let name = match (settings >> 8) & 0x0f {
            0 => "Single point",
            1 => "Zone",
            2 => "Wide/Tracking",
            _ => return None,
        };
        Some(name.to_owned())
    }

    fn faces(&self, frame: Frame) -> Vec<Area> {
        let Some(values) = self.shorts(FACE_POSITIONS) else {
            return Vec::new();
        };
        values
            .chunks_exact(4)
            .take(MAX_FACES)
            .filter_map(|corners| {
                let first = frame.normalize(corners[0], corners[1])?;
                let second = frame.normalize(corners[2], corners[3])?;
                Some(Area {
                    left: first.x.min(second.x),
                    top: first.y.min(second.y),
                    right: first.x.max(second.x),
                    bottom: first.y.max(second.y),
                })
            })
            .filter(|area| area.right > area.left && area.bottom > area.top)
            .collect()
    }
}

#[cfg(test)]
pub mod fixtures {
    pub fn exif_jpeg(fields: &[exif::Field]) -> Vec<u8> {
        let mut writer = exif::experimental::Writer::new();
        for field in fields {
            writer.push_field(field);
        }
        let mut tiff = std::io::Cursor::new(Vec::new());
        if writer.write(&mut tiff, true).is_err() {
            return Vec::new();
        }
        let tiff = tiff.into_inner();
        let length = u16::try_from(tiff.len() + 8).unwrap_or(u16::MAX);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend_from_slice(&length.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        jpeg
    }

    pub enum Data {
        Short(Vec<u16>),
        Long(u32),
    }

    pub fn fuji_note(entries: &[(u16, Data)]) -> Vec<u8> {
        let ifd = 12u32;
        let table_len = 2 + entries.len() * 12 + 4;
        let mut extra_at = ifd as usize + table_len;
        let mut table = Vec::new();
        let mut extra = Vec::new();
        table.extend_from_slice(&u16::try_from(entries.len()).unwrap_or(0).to_le_bytes());
        for (tag, data) in entries {
            table.extend_from_slice(&tag.to_le_bytes());
            let (kind, count, bytes): (u16, u32, Vec<u8>) = match data {
                Data::Short(values) => (
                    3,
                    u32::try_from(values.len()).unwrap_or(0),
                    values
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect(),
                ),
                Data::Long(value) => (4, 1, value.to_le_bytes().to_vec()),
            };
            table.extend_from_slice(&kind.to_le_bytes());
            table.extend_from_slice(&count.to_le_bytes());
            if bytes.len() <= 4 {
                let mut inline = bytes.clone();
                inline.resize(4, 0);
                table.extend_from_slice(&inline);
            } else {
                table.extend_from_slice(&u32::try_from(extra_at).unwrap_or(0).to_le_bytes());
                extra_at += bytes.len();
                extra.extend_from_slice(&bytes);
            }
        }
        table.extend_from_slice(&0u32.to_le_bytes());
        let mut note = b"FUJIFILM".to_vec();
        note.extend_from_slice(&ifd.to_le_bytes());
        note.extend_from_slice(&table);
        note.extend_from_slice(&extra);
        note
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{Data, exif_jpeg as jpeg_with, fuji_note};
    use super::*;
    use exif::{Field, Rational, SRational};

    fn primary(tag: Tag, value: Value) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value,
        }
    }

    fn fuji_shot(note: Vec<u8>, orientation: u16) -> Vec<u8> {
        jpeg_with(&[
            primary(Tag::Make, Value::Ascii(vec![b"FUJIFILM".to_vec()])),
            primary(Tag::Model, Value::Ascii(vec![b"X-T5".to_vec()])),
            primary(Tag::Orientation, Value::Short(vec![orientation])),
            primary(
                Tag::LensModel,
                Value::Ascii(vec![b"XF23mmF2 R WR".to_vec()]),
            ),
            primary(
                Tag::ExposureTime,
                Value::Rational(vec![Rational { num: 1, denom: 125 }]),
            ),
            primary(
                Tag::FNumber,
                Value::Rational(vec![Rational { num: 2, denom: 1 }]),
            ),
            primary(Tag::PhotographicSensitivity, Value::Short(vec![3200])),
            primary(
                Tag::ExposureBiasValue,
                Value::SRational(vec![SRational { num: -1, denom: 3 }]),
            ),
            primary(Tag::ExposureProgram, Value::Short(vec![3])),
            primary(
                Tag::FocalLength,
                Value::Rational(vec![Rational { num: 23, denom: 1 }]),
            ),
            primary(Tag::FocalLengthIn35mmFilm, Value::Short(vec![35])),
            primary(Tag::PixelXDimension, Value::Long(vec![6000])),
            primary(Tag::PixelYDimension, Value::Long(vec![4000])),
            primary(Tag::MakerNote, Value::Undefined(note, 0)),
        ])
    }

    fn sample_note() -> Vec<u8> {
        fuji_note(&[
            (FOCUS_MODE, Data::Short(vec![0])),
            (FOCUS_PIXEL, Data::Short(vec![3000, 1000])),
            (FOCUS_SETTINGS, Data::Long(0x0000_0002)),
            (GRAIN_ROUGHNESS, Data::Long(32)),
            (GRAIN_SIZE, Data::Short(vec![16])),
            (BLUR_WARNING, Data::Short(vec![0])),
            (FOCUS_WARNING, Data::Short(vec![1])),
            (EXPOSURE_WARNING, Data::Short(vec![0])),
            (FILM_MODE, Data::Short(vec![0x600])),
            (DYNAMIC_RANGE_SETTING, Data::Short(vec![1])),
            (DEVELOPMENT_DYNAMIC_RANGE, Data::Short(vec![200])),
            (FACE_POSITIONS, Data::Short(vec![1200, 800, 1800, 1600])),
        ])
    }

    #[test]
    fn reads_exposure_and_lens_from_exif() {
        let info = shot_info(&fuji_shot(sample_note(), 1));
        assert_eq!(info.camera.as_deref(), Some("Fujifilm X-T5"));
        assert_eq!(info.lens.as_deref(), Some("XF23mmF2 R WR"));
        assert_eq!(info.exposure_time.as_deref(), Some("1/125"));
        assert_eq!(info.f_number, Some(2.0));
        assert_eq!(info.iso, Some(3200));
        assert_eq!(info.program.as_deref(), Some("Aperture priority"));
        assert_eq!(info.focal_length, Some(23.0));
        assert_eq!(info.focal_length_35mm, Some(35));
        assert!(
            info.exposure_bias
                .is_some_and(|bias| (bias + 1.0 / 3.0).abs() < 1e-9)
        );
    }

    #[test]
    fn reads_fujifilm_maker_notes() {
        let info = shot_info(&fuji_shot(sample_note(), 1));
        assert_eq!(info.film_simulation.as_deref(), Some("Classic Chrome"));
        assert_eq!(info.dynamic_range.as_deref(), Some("DR200"));
        assert_eq!(info.grain.as_deref(), Some("Weak, small"));
        assert_eq!(info.focus_mode.as_deref(), Some("AF-C"));
        assert_eq!(info.af_area.as_deref(), Some("Single point"));
        assert_eq!(info.focus_point, Some(Point { x: 0.5, y: 0.25 }));
        assert_eq!(
            info.faces,
            vec![Area {
                left: 0.2,
                top: 0.2,
                right: 0.3,
                bottom: 0.4
            }]
        );
        assert_eq!(info.focus_warning, Some(true));
        assert_eq!(info.blur_warning, Some(false));
        assert_eq!(info.exposure_warning, Some(false));
    }

    #[test]
    fn rotates_the_focus_point_with_the_photo() {
        let info = shot_info(&fuji_shot(sample_note(), 6));
        assert_eq!(info.focus_point, Some(Point { x: 0.75, y: 0.5 }));
        let info = shot_info(&fuji_shot(sample_note(), 8));
        assert_eq!(info.focus_point, Some(Point { x: 0.25, y: 0.5 }));
    }

    #[test]
    fn monochrome_simulations_come_from_saturation() {
        let note = fuji_note(&[
            (SATURATION, Data::Short(vec![0x501])),
            (FILM_MODE, Data::Short(vec![0x600])),
        ]);
        let info = shot_info(&fuji_shot(note, 1));
        assert_eq!(info.film_simulation.as_deref(), Some("Acros + R"));
    }

    #[test]
    fn focus_point_outside_the_image_is_dropped() {
        let note = fuji_note(&[(FOCUS_PIXEL, Data::Short(vec![9000, 100]))]);
        assert_eq!(shot_info(&fuji_shot(note, 1)).focus_point, None);
    }

    #[test]
    fn malformed_maker_notes_never_panic() {
        let note = sample_note();
        for cut in 0..note.len() {
            let _ = shot_info(&fuji_shot(note[..cut].to_vec(), 1));
        }
        let mut huge = b"FUJIFILM".to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(shot_info(&fuji_shot(huge, 1)).film_simulation, None);
        let mut lying = b"FUJIFILM".to_vec();
        lying.extend_from_slice(&12u32.to_le_bytes());
        lying.extend_from_slice(&1u16.to_le_bytes());
        lying.extend_from_slice(&FACE_POSITIONS.to_le_bytes());
        lying.extend_from_slice(&3u16.to_le_bytes());
        lying.extend_from_slice(&u32::MAX.to_le_bytes());
        lying.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(shot_info(&fuji_shot(lying, 1)).faces.is_empty());
    }

    #[test]
    fn photos_without_exif_have_no_info() {
        assert_eq!(shot_info(b"not a jpeg"), ShotInfo::default());
        assert_eq!(shot_info(&[0xFF, 0xD8, 0xFF, 0xD9]), ShotInfo::default());
    }

    #[test]
    fn long_and_short_exposures_read_naturally() {
        let shot = |num, denom| {
            shot_info(&jpeg_with(&[primary(
                Tag::ExposureTime,
                Value::Rational(vec![Rational { num, denom }]),
            )]))
            .exposure_time
        };
        assert_eq!(shot(1, 4000).as_deref(), Some("1/4000"));
        assert_eq!(shot(10, 4).as_deref(), Some("2.5"));
        assert_eq!(shot(30, 1).as_deref(), Some("30"));
        assert_eq!(shot(0, 1), None);
    }
}
