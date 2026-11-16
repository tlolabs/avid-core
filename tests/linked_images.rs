//! Static native image decoders, with deterministic input bytes and no helpers.
use avid_core::*;
use std::{fs, path::Path};
fn bmp(path: &Path) {
    let (width, height) = (32u32, 16u32);
    let pixels = width * height * 3;
    let mut bytes = b"BM".to_vec();
    bytes.extend((54 + pixels).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(54u32.to_le_bytes());
    bytes.extend(40u32.to_le_bytes());
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(24u16.to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(pixels.to_le_bytes());
    bytes.extend([0; 16]);
    bytes.extend([0, 0, 255].repeat((width * height) as usize));
    fs::write(path, bytes).unwrap();
}
fn tiff(path: &Path) {
    let (width, height) = (32u32, 16u32);
    let pixels = width * height * 3;
    let mut bytes = b"II\x2a\0".to_vec();
    bytes.extend(8u32.to_le_bytes());
    bytes.extend(10u16.to_le_bytes());
    // One uncompressed chunky RGB strip. SHORT scalar values occupy the low
    // two bytes of the TIFF value field; the remaining two bytes stay zero.
    for (tag, kind, count, value) in [
        (256u16, 4u16, 1u32, width),
        (257, 4, 1, height),
        (258, 3, 3, 134),
        (259, 3, 1, 1),
        (262, 3, 1, 2),
        (273, 4, 1, 140),
        (277, 3, 1, 3),
        (278, 4, 1, height),
        (279, 4, 1, pixels),
        (284, 3, 1, 1),
    ] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(kind.to_le_bytes());
        bytes.extend(count.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0u32.to_le_bytes());
    for _ in 0..3 {
        bytes.extend(8u16.to_le_bytes());
    }
    assert_eq!(bytes.len(), 140);
    bytes.extend([255, 0, 0].repeat((width * height) as usize));
    fs::write(path, bytes).unwrap();
}
#[test]
fn bmp_and_tiff_inspection_preview_and_decoded_pixels() {
    let root = tempfile::tempdir().unwrap();
    let renderer = Renderer::new();
    let token = CancellationToken::default();
    for (name, make) in [
        ("cover.bmp", bmp as fn(&Path)),
        ("cover.tiff", tiff as fn(&Path)),
    ] {
        let image = root.path().join(name);
        make(&image);
        assert_eq!(renderer.inspect_image(&image, &token).unwrap(), (32, 16));
        for composition in [Composition::Fitted, Composition::SquarePadded] {
            let output = root.path().join("preview.png");
            renderer
                .preview(
                    &PreviewRequest {
                        image: image.clone(),
                        output: output.clone(),
                        settings: RenderSettings {
                            width: 160,
                            height: 90,
                            composition,
                            ..renderer.default_settings()
                        },
                        protected_paths: vec![],
                    },
                    &token,
                    &(),
                )
                .unwrap();
            let mut count = 0;
            let control = avid_media_bridge::Control {
                cancelled: &|| false,
                progress: &|_| {},
            };
            avid_media_bridge::decode_video(&output, &control, &mut |frame| {
                assert_eq!((frame.width, frame.height), (160, 90));
                let at =
                    frame.stride * (frame.height as usize / 2) + (frame.width as usize / 2) * 3;
                let rgb = &frame.pixels[at..at + 3];
                assert!(
                    i32::from(rgb[0]) - i32::from(rgb[2]) > 100,
                    "{name}: {rgb:?}"
                );
                count += 1;
                true
            })
            .unwrap();
            assert_eq!(count, 1);
        }
    }
}
