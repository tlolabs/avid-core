// SPDX-FileCopyrightText: Thomas Lothian
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{inspect_png, ImageOptimizationOptions, OptimizationEffort};
use crate::{CancellationToken, Error, Result};
use std::{io::Cursor, num::NonZeroU8};

pub(super) fn optimize(
    input: &[u8],
    settings: &ImageOptimizationOptions,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    inspect_png(input)?;
    require_opaque(input, cancel)?;
    cancel.check()?;
    let mut options = oxipng::Options::from_preset(settings.effort.png_level());
    options.interlace = None;
    options.strip = if settings.strip_png_metadata {
        oxipng::StripChunks::All
    } else {
        oxipng::StripChunks::None
    };
    options.optimize_alpha = false;
    options.scale_16 = false;
    // Keep the original compressed pixels as a fallback, applying the requested
    // stripping first. An optimizer may produce a larger result after stripping;
    // falling back to unstripped input would ignore the user's metadata setting.
    let baseline = if settings.strip_png_metadata {
        strip_metadata(input)
    } else {
        input.to_vec()
    };
    let candidate = oxipng::optimize_from_memory(&baseline, &options).map_err(codec_error)?;
    let mut best = if candidate.len() < baseline.len() {
        candidate
    } else {
        baseline
    };
    cancel.check()?;
    if settings.png_zopfli {
        let iterations = match settings.effort {
            OptimizationEffort::Fast => 1,
            OptimizationEffort::Normal => 5,
            OptimizationEffort::Extra => 15,
            OptimizationEffort::Insane => 30,
        };
        options.deflate = oxipng::Deflaters::Zopfli {
            iterations: NonZeroU8::new(iterations).unwrap(),
        };
        let candidate = oxipng::optimize_from_memory(&best, &options).map_err(codec_error)?;
        if candidate.len() < best.len() {
            best = candidate;
        }
        cancel.check()?;
    }
    Ok(best)
}

fn codec_error(error: impl std::fmt::Display) -> Error {
    Error::ImageOptimization(format!("PNG: {error}"))
}

fn require_opaque(input: &[u8], cancel: &CancellationToken) -> Result<()> {
    // Validate the complete source before metadata stripping can remove evidence
    // of corrupt ancillary chunks. The decoder defaults tolerate those CRC
    // failures and skip the zlib checksum, which is too permissive here.
    let mut decode_options = png::DecodeOptions::default();
    decode_options.set_skip_ancillary_crc_failures(false);
    decode_options.set_ignore_adler32(false);
    let mut decoder = png::Decoder::new_with_options(Cursor::new(input), decode_options);
    decoder.set_limits(png::Limits {
        bytes: 512 * 1024 * 1024,
    });
    // Expand palettes, low-bit grayscale and tRNS into explicit 8/16-bit samples.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(codec_error)?;
    let (color, depth) = reader.output_color_type();
    let channels = color.samples();
    let sample_bytes = if depth == png::BitDepth::Sixteen {
        2
    } else {
        1
    };
    let alpha = matches!(color, png::ColorType::Rgba | png::ColorType::GrayscaleAlpha);
    // Adam7 requires whole-frame reconstruction. The dimension checks already
    // cap this allocation at 50M pixels, at most 8 bytes per expanded pixel.
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(reader.output_buffer_size())
        .map_err(|_| Error::ImageOptimization("PNG pixel allocation failed".into()))?;
    pixels.resize(reader.output_buffer_size(), 0);
    let frame = reader.next_frame(&mut pixels).map_err(codec_error)?;
    reader.finish().map_err(codec_error)?;
    cancel.check()?;
    if alpha {
        let pixel_bytes = channels * sample_bytes;
        for batch in pixels[..frame.buffer_size()].chunks(65_536 * pixel_bytes) {
            cancel.check()?;
            if batch.chunks_exact(pixel_bytes).any(|pixel| {
                pixel[pixel_bytes - sample_bytes..]
                    .iter()
                    .any(|alpha| *alpha != 255)
            }) {
                return Err(Error::InvalidInput(
                    "Album artwork PNG must be opaque; transparent pixels are unsupported".into(),
                ));
            }
        }
    }
    Ok(())
}

// The complete file (including CRCs and compressed pixels) has already been
// decoded and validated, and every pixel in the image was proven opaque.
// Keep only critical chunks when stripping metadata.
fn strip_metadata(input: &[u8]) -> Vec<u8> {
    let mut output = input[..8].to_vec();
    let mut position = 8;
    while position < input.len() {
        let length = u32::from_be_bytes(input[position..position + 4].try_into().unwrap()) as usize;
        let kind = &input[position + 4..position + 8];
        let end = position + length + 12;
        if kind[0].is_ascii_uppercase() {
            output.extend_from_slice(&input[position..end]);
        }
        position = end;
    }
    output
}
