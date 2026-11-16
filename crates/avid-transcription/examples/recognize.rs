use avid_transcription::*;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: recognize CACHE GGML_TINY_EN_MODEL LOCAL_AUDIO".into());
    }
    let store = ModelStore::new(&args[0])?;
    let manifest = ModelManifest::tiny_english();
    store.import(&manifest, args[1].as_ref(), &|| false)?;
    let lease = store.lease(&manifest, &|| false)?;
    let transcript = recognize_audio(
        &lease,
        args[2].as_ref(),
        &RecognitionOptions {
            language: Some("en"),
            ..Default::default()
        },
        &|| false,
        &|value| eprintln!("{} {:?}", value.stage, value.fraction),
    )?;
    println!("{}", serde_json::to_string_pretty(&transcript)?);
    Ok(())
}
