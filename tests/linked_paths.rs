//! Literal source paths must not become image-sequence patterns or implicit
//! inputs outside Core's protected source list. No executable is used here.
use avid_core::{CancellationToken, PreviewRequest, RenderSettings, Renderer};
use avid_media_bridge::{decode_video, probe, Control};
use std::{fs, path::Path};

fn image(path: &Path, width: u32, height: u32, color: [u8; 3]) {
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&color.repeat((width * height) as usize))
        .unwrap();
}
fn control() -> Control<'static> {
    Control {
        cancelled: &|| false,
        progress: &|_| {},
    }
}

#[test]
fn percent_number_source_names_are_literal_and_keep_all_sources_intact() {
    let directory = tempfile::tempdir().unwrap();
    let literal = directory.path().join("art%03d.png");
    let numbered = directory.path().join("art001.png");
    image(&literal, 32, 16, [255, 0, 0]);
    image(&numbered, 64, 32, [0, 0, 255]);
    let literal_bytes = fs::read(&literal).unwrap();
    let numbered_bytes = fs::read(&numbered).unwrap();
    let renderer = Renderer::new();
    let token = CancellationToken::default();
    assert_eq!(renderer.inspect_image(&literal, &token).unwrap(), (32, 16));
    let metadata = probe(&literal, &control()).unwrap();
    assert_eq!((metadata.width, metadata.height), (32, 16));
    let output = directory.path().join("preview.png");
    let request = PreviewRequest {
        image: literal.clone(),
        output: output.clone(),
        settings: RenderSettings {
            width: 160,
            height: 90,
            ..Default::default()
        },
        protected_paths: vec![numbered.clone()],
    };
    renderer.preview(&request, &token, &()).unwrap();
    let mut color = None;
    decode_video(&output, &control(), &mut |frame| {
        let center = frame.stride * (frame.height as usize / 2) + frame.width as usize / 2 * 3;
        color = Some(<[u8; 3]>::try_from(&frame.pixels[center..center + 3]).unwrap());
        false
    })
    .unwrap();
    let color = color.unwrap();
    assert!(
        i32::from(color[0]) - i32::from(color[2]) > 150,
        "the literal red source must be used, not numbered blue input: {color:?}"
    );
    assert_eq!(fs::read(&literal).unwrap(), literal_bytes);
    assert_eq!(fs::read(&numbered).unwrap(), numbered_bytes);

    // Source/destination alias rejection still protects the literal filename.
    let alias_request = PreviewRequest {
        output: literal.clone(),
        ..request
    };
    assert!(renderer.preview(&alias_request, &token, &()).is_err());
    assert_eq!(fs::read(&literal).unwrap(), literal_bytes);
    assert_eq!(fs::read(&numbered).unwrap(), numbered_bytes);
}

#[test]
fn missing_literal_percent_path_never_uses_a_numbered_neighbor() {
    let directory = tempfile::tempdir().unwrap();
    let numbered = directory.path().join("art001.png");
    image(&numbered, 64, 32, [0, 0, 255]);
    let numbered_bytes = fs::read(&numbered).unwrap();
    let missing = directory.path().join("art%03d.png");
    assert!(Renderer::new()
        .inspect_image(&missing, &CancellationToken::default())
        .is_err());
    // Some image2 builds return an empty, unprobed video header for a missing
    // literal path. It must never identify the numbered neighbor's dimensions.
    if let Ok(metadata) = probe(&missing, &control()) {
        assert_eq!((metadata.width, metadata.height), (0, 0));
        assert!(!metadata.has_audio);
    }
    assert!(!missing.exists());
    assert_eq!(fs::read(&numbered).unwrap(), numbered_bytes);
}
