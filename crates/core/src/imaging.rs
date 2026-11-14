use std::io::Cursor;

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType as ResizeFilter, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, GrayImage, RgbImage};
use jpeg_decoder::{Decoder, PixelFormat};
use thiserror::Error;

pub const THUMB_EDGE: u32 = 360;
pub const PREVIEW_EDGE: u32 = 2560;
const JPEG_QUALITY: u8 = 85;
const MAX_DECODE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ImagingError {
    #[error("cannot decode JPEG: {0}")]
    Decode(#[from] jpeg_decoder::Error),
    #[error("cannot encode JPEG: {0}")]
    Encode(#[from] image::ImageError),
    #[error("unsupported JPEG pixel format")]
    PixelFormat,
    #[error("JPEG has no image data")]
    Empty,
    #[error("cannot resize: {0}")]
    Resize(String),
}

impl From<fast_image_resize::ImageBufferError> for ImagingError {
    fn from(error: fast_image_resize::ImageBufferError) -> Self {
        ImagingError::Resize(error.to_string())
    }
}

impl From<fast_image_resize::ResizeError> for ImagingError {
    fn from(error: fast_image_resize::ResizeError) -> Self {
        ImagingError::Resize(error.to_string())
    }
}

pub struct Rendered {
    pub thumb: Vec<u8>,
    pub preview: Vec<u8>,
    pub dhash: u64,
    pub width: u32,
    pub height: u32,
}

fn fit(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= edge {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(edge) / f64::from(long);
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (scaled(width), scaled(height))
}

fn decode_at_least(jpeg: &[u8], edge: u32) -> Result<RgbImage, ImagingError> {
    let mut decoder = Decoder::new(Cursor::new(jpeg));
    decoder.set_max_decoding_buffer_size(MAX_DECODE_BYTES);
    decoder.read_info()?;
    let info = decoder.info().ok_or(ImagingError::Empty)?;
    let (target_w, target_h) = fit(u32::from(info.width), u32::from(info.height), edge);
    let clamp = |side: u32| u16::try_from(side).unwrap_or(u16::MAX);
    let (width, height) = decoder.scale(clamp(target_w), clamp(target_h))?;
    let pixels = decoder.decode()?;
    let (width, height) = (u32::from(width), u32::from(height));
    let rgb = match info.pixel_format {
        PixelFormat::RGB24 => pixels,
        PixelFormat::L8 => pixels.iter().flat_map(|&v| [v, v, v]).collect(),
        PixelFormat::L16 => pixels
            .chunks_exact(2)
            .flat_map(|pair| [pair[0], pair[0], pair[0]])
            .collect(),
        PixelFormat::CMYK32 => pixels
            .chunks_exact(4)
            .flat_map(|cmyk| {
                let k = u16::from(cmyk[3]);
                let channel = |c: u8| ((u16::from(c) * k) / 255) as u8;
                [channel(cmyk[0]), channel(cmyk[1]), channel(cmyk[2])]
            })
            .collect(),
    };
    RgbImage::from_raw(width, height, rgb).ok_or(ImagingError::PixelFormat)
}

pub fn orient(image: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

fn resize_to(image: &RgbImage, edge: u32) -> Result<RgbImage, ImagingError> {
    let (width, height) = fit(image.width(), image.height(), edge);
    if (width, height) == image.dimensions() {
        return Ok(image.clone());
    }
    let source = Image::from_vec_u8(
        image.width(),
        image.height(),
        image.as_raw().clone(),
        PixelType::U8x3,
    )?;
    let mut target = Image::new(width, height, PixelType::U8x3);
    Resizer::new().resize(
        &source,
        &mut target,
        &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(ResizeFilter::Bilinear)),
    )?;
    RgbImage::from_raw(width, height, target.into_vec()).ok_or(ImagingError::PixelFormat)
}

pub fn encode(image: &RgbImage) -> Result<Vec<u8>, ImagingError> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY).encode_image(image)?;
    Ok(bytes)
}

pub fn dhash(image: &RgbImage) -> u64 {
    let gray: GrayImage = imageops::grayscale(image);
    let small = imageops::resize(&gray, 9, 8, FilterType::Triangle);
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            hash = (hash << 1) | u64::from(left > right);
        }
    }
    hash
}

pub fn render(jpeg: &[u8], orientation: u16) -> Result<Rendered, ImagingError> {
    let decoded = decode_at_least(jpeg, PREVIEW_EDGE)?;
    let upright = orient(DynamicImage::ImageRgb8(decoded), orientation).into_rgb8();
    let preview = resize_to(&upright, PREVIEW_EDGE)?;
    let thumb = resize_to(&preview, THUMB_EDGE)?;
    Ok(Rendered {
        dhash: dhash(&thumb),
        thumb: encode(&thumb)?,
        preview: encode(&preview)?,
        width: preview.width(),
        height: preview.height(),
    })
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    pub fn gradient_jpeg(width: u32, height: u32, seed: u8) -> Vec<u8> {
        let image = RgbImage::from_fn(width, height, |x, y| {
            let wave = ((x * 7 + y * 3) % 255) as u8;
            image::Rgb([
                wave.wrapping_add(seed),
                ((y * 255) / height.max(1)) as u8,
                ((x * 255) / width.max(1)) as u8 ^ seed,
            ])
        });
        encode(&image).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::gradient_jpeg;
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_never_upscales() {
        assert_eq!(fit(6000, 4000, 2560), (2560, 1707));
        assert_eq!(fit(4000, 6000, 360), (240, 360));
        assert_eq!(fit(100, 50, 360), (100, 50));
    }

    #[test]
    fn renders_thumb_and_preview_within_bounds() -> Result<(), ImagingError> {
        let rendered = render(&gradient_jpeg(3000, 2000, 1), 1)?;
        assert_eq!((rendered.width, rendered.height), (2560, 1707));
        assert!(rendered.thumb.starts_with(&[0xFF, 0xD8]));
        assert!(rendered.preview.starts_with(&[0xFF, 0xD8]));
        Ok(())
    }

    #[test]
    fn rotates_portrait_orientation() -> Result<(), ImagingError> {
        let rendered = render(&gradient_jpeg(600, 400, 1), 6)?;
        assert_eq!((rendered.width, rendered.height), (400, 600));
        Ok(())
    }

    #[test]
    fn dhash_is_stable_for_the_same_picture_and_differs_for_another() -> Result<(), ImagingError> {
        let first = render(&gradient_jpeg(800, 600, 1), 1)?.dhash;
        let again = render(&gradient_jpeg(800, 600, 1), 1)?.dhash;
        let flipped = render(&gradient_jpeg(800, 600, 1), 2)?.dhash;
        assert_eq!(first, again);
        assert!(hamming(first, flipped) > 12);
        Ok(())
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(render(&[0xFF, 0xD8, 0xFF, 0xC0, 0, 0], 1).is_err());
        assert!(render(&[], 1).is_err());
    }
}
