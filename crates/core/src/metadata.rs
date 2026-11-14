use std::io::Cursor;

use exif::{In, Reader, Tag, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureInfo {
    pub capture_ms: Option<i64>,
    pub orientation: u16,
}

impl Default for CaptureInfo {
    fn default() -> Self {
        CaptureInfo {
            capture_ms: None,
            orientation: 1,
        }
    }
}

pub fn capture_info(jpeg: &[u8]) -> CaptureInfo {
    let Ok(exif) = Reader::new().read_from_container(&mut Cursor::new(jpeg)) else {
        return CaptureInfo::default();
    };
    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| (1..=8).contains(value))
        .unwrap_or(1);
    let ascii = |tag: Tag| {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|field| match &field.value {
                Value::Ascii(parts) => parts
                    .first()
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
                _ => None,
            })
    };
    let capture_ms = ascii(Tag::DateTimeOriginal)
        .and_then(|text| parse_exif_datetime(&text))
        .map(|seconds| {
            let fraction = ascii(Tag::SubSecTimeOriginal)
                .map(|sub| sub_second_ms(&sub))
                .unwrap_or(0);
            seconds * 1000 + fraction
        });
    CaptureInfo {
        capture_ms,
        orientation,
    }
}

fn sub_second_ms(text: &str) -> i64 {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect();
    digits.parse().unwrap_or(0)
}

pub fn parse_exif_datetime(text: &str) -> Option<i64> {
    let bytes = text.trim().as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        std::str::from_utf8(bytes.get(range)?).ok()?.parse().ok()
    };
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_datetime_as_local_seconds() {
        assert_eq!(parse_exif_datetime("1970:01:01 00:00:00"), Some(0));
        assert_eq!(
            parse_exif_datetime("2026:10:12 08:42:10"),
            Some(1_791_794_530)
        );
        assert_eq!(
            parse_exif_datetime("2000:02:29 12:00:00"),
            Some(951_825_600)
        );
    }

    #[test]
    fn rejects_malformed_datetime() {
        assert_eq!(parse_exif_datetime(""), None);
        assert_eq!(parse_exif_datetime("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif_datetime("2026:13:01 00:00:00"), None);
        assert_eq!(parse_exif_datetime("2026-10-12"), None);
        assert_eq!(parse_exif_datetime("abcd:ef:gh ij:kl:mn"), None);
    }

    #[test]
    fn sub_seconds_are_padded_to_milliseconds() {
        assert_eq!(sub_second_ms("5"), 500);
        assert_eq!(sub_second_ms("05"), 50);
        assert_eq!(sub_second_ms("123456"), 123);
        assert_eq!(sub_second_ms(""), 0);
    }

    fn jpeg_with_exif(fields: &[exif::Field]) -> Vec<u8> {
        let mut writer = exif::experimental::Writer::new();
        for field in fields {
            writer.push_field(field);
        }
        let mut tiff = std::io::Cursor::new(Vec::new());
        if writer.write(&mut tiff, false).is_err() {
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

    fn ascii_field(tag: Tag, text: &str) -> exif::Field {
        exif::Field {
            tag,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![text.as_bytes().to_vec()]),
        }
    }

    #[test]
    fn reads_capture_time_sub_seconds_and_orientation() {
        let jpeg = jpeg_with_exif(&[
            ascii_field(Tag::DateTimeOriginal, "2026:10:12 08:42:10"),
            ascii_field(Tag::SubSecTimeOriginal, "25"),
            exif::Field {
                tag: Tag::Orientation,
                ifd_num: In::PRIMARY,
                value: Value::Short(vec![6]),
            },
        ]);
        assert_eq!(
            capture_info(&jpeg),
            CaptureInfo {
                capture_ms: Some(1_791_794_530_250),
                orientation: 6
            }
        );
    }

    #[test]
    fn ignores_modification_date_without_capture_date() {
        let jpeg = jpeg_with_exif(&[ascii_field(Tag::DateTime, "2026:10:12 08:42:10")]);
        assert_eq!(capture_info(&jpeg).capture_ms, None);
    }

    #[test]
    fn missing_exif_defaults_to_upright_without_time() {
        assert_eq!(
            capture_info(&[0xFF, 0xD8, 0xFF, 0xD9]),
            CaptureInfo {
                capture_ms: None,
                orientation: 1
            }
        );
    }
}
