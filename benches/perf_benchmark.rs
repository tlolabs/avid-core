use avid_core::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn run_cmd(program: &Path, args: &[&str]) {
    let out = Command::new(program).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn create_fixtures(renderer: &Renderer, root: &Path) -> (Vec<PathBuf>, PathBuf) {
    let mut images = Vec::new();
    for i in 0..5 {
        let art = root.join(format!("art_{i}.png"));
        run_cmd(
            renderer.tools().ffmpeg(),
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c=blue:s=640x480"),
                "-frames:v",
                "1",
                "-y",
                art.to_str().unwrap(),
            ],
        );
        images.push(art);
    }
    let audio = root.join("track.wav");
    run_cmd(
        renderer.tools().ffmpeg(),
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1:sample_rate=44100",
            "-c:a",
            "pcm_s16le",
            "-y",
            audio.to_str().unwrap(),
        ],
    );
    (images, audio)
}

struct Stats {
    runs: Vec<Duration>,
}

impl Stats {
    fn record<F: FnMut()>(mut f: F, iterations: usize) -> Self {
        let mut runs = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let start = Instant::now();
            f();
            runs.push(start.elapsed());
        }
        runs.sort();
        Self { runs }
    }

    fn median_ms(&self) -> f64 {
        let mid = self.runs.len() / 2;
        if self.runs.len() % 2 == 0 {
            (self.runs[mid - 1].as_secs_f64() + self.runs[mid].as_secs_f64()) / 2.0 * 1000.0
        } else {
            self.runs[mid].as_secs_f64() * 1000.0
        }
    }

    fn mean_ms(&self) -> f64 {
        let total: f64 = self.runs.iter().map(|d| d.as_secs_f64()).sum();
        (total / self.runs.len() as f64) * 1000.0
    }

    fn min_ms(&self) -> f64 {
        self.runs.first().unwrap().as_secs_f64() * 1000.0
    }

    fn max_ms(&self) -> f64 {
        self.runs.last().unwrap().as_secs_f64() * 1000.0
    }
}

fn print_metric(name: &str, stats: &Stats) {
    println!(
        "{:<45} | min: {:>7.2}ms | median: {:>7.2}ms | mean: {:>7.2}ms | max: {:>7.2}ms",
        name,
        stats.min_ms(),
        stats.median_ms(),
        stats.mean_ms(),
        stats.max_ms()
    );
}

fn main() {
    println!("=== AVID Core Performance Benchmark ===");
    let token = CancellationToken::default();
    let tools = MediaTools::discover(ToolDiscovery::default(), &token)
        .expect("FFmpeg and ffprobe must be available");
    let ffmpeg_path = tools.ffmpeg().to_path_buf();
    let ffprobe_path = tools.ffprobe().to_path_buf();

    // 1. Tool discovery and validation
    let stats_from_paths = Stats::record(
        || {
            let _ = MediaTools::from_paths(&ffmpeg_path, &ffprobe_path, &token).unwrap();
        },
        10,
    );
    print_metric("MediaTools::from_paths (2x -version)", &stats_from_paths);

    let stats_discover = Stats::record(
        || {
            let _ = MediaTools::discover(ToolDiscovery::default(), &token).unwrap();
        },
        10,
    );
    print_metric("MediaTools::discover (PATH scan + 2x -version)", &stats_discover);

    // 2. Capabilities detection
    let renderer = Renderer::new(tools.clone());
    let stats_capabilities = Stats::record(
        || {
            let _ = renderer.capabilities(&token).unwrap();
        },
        10,
    );
    print_metric("Renderer::capabilities (ffmpeg -encoders)", &stats_capabilities);

    // Set up temp fixtures
    let temp_dir = tempfile::tempdir().unwrap();
    let (images, audio) = create_fixtures(&renderer, temp_dir.path());

    // 3. Probing image: Cold vs Cached
    let single_image = &images[0];
    let stats_inspect_cold = Stats::record(
        || {
            let r = Renderer::new(tools.clone());
            let _ = r.inspect_image(single_image, &token).unwrap();
        },
        10,
    );
    print_metric("Renderer::inspect_image (cold/uncached)", &stats_inspect_cold);

    let stats_inspect_cached = Stats::record(
        || {
            let _ = renderer.inspect_image(single_image, &token).unwrap();
        },
        10,
    );
    print_metric("Renderer::inspect_image (cached)", &stats_inspect_cached);

    // 4. Probing audio: Cold vs Cached
    let stats_probe_cold = Stats::record(
        || {
            let r = Renderer::new(tools.clone());
            let _ = r.probe_audio_duration(&audio, &token).unwrap();
        },
        10,
    );
    print_metric("Renderer::probe_audio_duration (cold/uncached)", &stats_probe_cold);

    let stats_probe_cached = Stats::record(
        || {
            let _ = renderer.probe_audio_duration(&audio, &token).unwrap();
        },
        10,
    );
    print_metric("Renderer::probe_audio_duration (cached)", &stats_probe_cached);

    // 5. Multi-image probing (5 images)
    let stats_inspect_5 = Stats::record(
        || {
            for img in &images {
                let _ = renderer.inspect_image(img, &token).unwrap();
            }
        },
        10,
    );
    print_metric("Inspect 5 images (cached after first)", &stats_inspect_5);

    // 6. Preview rendering
    let preview_out = temp_dir.path().join("preview.png");
    let preview_req = PreviewRequest {
        image: single_image.clone(),
        output: preview_out.clone(),
        settings: RenderSettings {
            width: 320,
            height: 240,
            ..Default::default()
        },
        protected_paths: vec![],
    };
    let stats_preview = Stats::record(
        || {
            let _ = renderer.preview(&preview_req, &token, &()).unwrap();
        },
        5,
    );
    print_metric("Renderer::preview (320x240 PNG)", &stats_preview);

    // 7. Video export - Simple mode
    let video_out_simple = temp_dir.path().join("out_simple.mp4");
    let render_req_simple = RenderRequest {
        input: Input::Single {
            image: single_image.clone(),
            audio: audio.clone(),
        },
        settings: RenderSettings {
            width: 320,
            height: 240,
            fps: 30,
            ..Default::default()
        },
        output: video_out_simple.clone(),
        protected_paths: vec![],
    };
    let stats_render_simple = Stats::record(
        || {
            let _ = renderer
                .render_with_mode(&render_req_simple, RenderMode::Simple, &token, &())
                .unwrap();
        },
        5,
    );
    print_metric("Renderer::render Simple (1s audio 320x240)", &stats_render_simple);

    // 8. Timeline export (3 clips)
    let timeline_clips = vec![
        Clip {
            id: "1".into(),
            image: images[0].clone(),
            audio: audio.clone(),
            duration_seconds: 0.5,
        },
        Clip {
            id: "2".into(),
            image: images[1].clone(),
            audio: audio.clone(),
            duration_seconds: 0.5,
        },
        Clip {
            id: "3".into(),
            image: images[2].clone(),
            audio: audio.clone(),
            duration_seconds: 0.5,
        },
    ];
    let timeline_out = temp_dir.path().join("timeline.mp4");
    let timeline_req = RenderRequest {
        input: Input::Timeline(Timeline::new(timeline_clips).unwrap()),
        settings: RenderSettings {
            width: 320,
            height: 240,
            fps: 30,
            ..Default::default()
        },
        output: timeline_out.clone(),
        protected_paths: vec![],
    };
    let stats_timeline = Stats::record(
        || {
            let _ = renderer
                .render_with_mode(&timeline_req, RenderMode::Simple, &token, &())
                .unwrap();
        },
        5,
    );
    print_metric("Renderer::render Timeline 3-clips Simple", &stats_timeline);

    println!("=== Benchmark Complete ===");
}
