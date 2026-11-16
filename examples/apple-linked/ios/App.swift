// Development integration test host. Not a distributable application.
// SPDX-License-Identifier: GPL-3.0-or-later
import UIKit
import AVFoundation
import Foundation

private enum TestFailure: Error { case failed(String) }
private func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw TestFailure.failed(message) }
}
private func little<T: FixedWidthInteger>(_ value: T, _ data: inout Data) {
    var value = value.littleEndian
    withUnsafeBytes(of: &value) { data.append(contentsOf: $0) }
}
private func fixtureAudio(_ url: URL) throws {
    let rate: UInt32 = 44_100
    var data = Data("RIFF".utf8)
    little(36 + rate * 2, &data)
    data.append(Data("WAVEfmt ".utf8)); little(UInt32(16), &data)
    little(UInt16(1), &data); little(UInt16(1), &data); little(rate, &data)
    little(rate * 2, &data); little(UInt16(2), &data); little(UInt16(16), &data)
    data.append(Data("data".utf8)); little(rate * 2, &data)
    for i in 0..<rate {
        let sample: Int16 = i < rate / 4 ? 0 : Int16(sin(Double(i) * 440 * 2 * .pi / Double(rate)) * 12_000)
        little(sample, &data)
    }
    try data.write(to: url, options: .atomic)
}
private func decodedCounts(_ url: URL) async throws -> [String: Any] {
    let asset = AVURLAsset(url: url)
    let duration = try await asset.load(.duration).seconds
    try require(abs(duration - 1) < 0.15, "MP4 duration mismatch: \(duration)")
    let video = try await asset.loadTracks(withMediaType: .video)
    let audio = try await asset.loadTracks(withMediaType: .audio)
    try require(video.count == 1 && audio.count == 1, "Expected one audio and one video track")
    let size = try await video[0].load(.naturalSize)
    try require(size == CGSize(width: 160, height: 90), "Video geometry mismatch: \(size)")
    let reader = try AVAssetReader(asset: asset)
    let videoOutput = AVAssetReaderTrackOutput(track: video[0], outputSettings: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
    let audioOutput = AVAssetReaderTrackOutput(track: audio[0], outputSettings: [
        AVFormatIDKey: kAudioFormatLinearPCM, AVLinearPCMBitDepthKey: 16,
        AVLinearPCMIsFloatKey: false, AVLinearPCMIsBigEndianKey: false])
    reader.add(videoOutput); reader.add(audioOutput)
    try require(reader.startReading(), "Decoder could not start")
    var frames = 0, samples = 0
    while let sample = videoOutput.copyNextSampleBuffer() {
        try require(CMSampleBufferGetImageBuffer(sample) != nil, "Video did not decode to pixels")
        frames += 1
    }
    while let sample = audioOutput.copyNextSampleBuffer() { samples += CMSampleBufferGetNumSamples(sample) }
    try require(reader.status == .completed, "Decode did not complete: \(String(describing: reader.error))")
    try require((23...25).contains(frames) && samples > 40_000, "Decoded media count mismatch")
    return ["duration_seconds": duration, "decoded_video_frames": frames, "decoded_audio_samples": samples]
}
private func runTests(directory: URL) async -> [String: Any] {
    let device = await MainActor.run { UIDevice.current.model }
    let systemVersion = await MainActor.run { UIDevice.current.systemVersion }
    var report: [String: Any] = ["schema": 1, "kind": "avid-linked-apple-host-test", "platform": "iOS",
                               "device": device, "system_version": systemVersion]
    do {
        let core = try LinkedCore()
        let image = directory.appendingPathComponent("artwork ü.png")
        let audio = directory.appendingPathComponent("initial silence ü.wav")
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 96, height: 64))
        let png = renderer.pngData { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 48, height: 64))
            UIColor.blue.setFill(); context.fill(CGRect(x: 48, y: 0, width: 48, height: 64))
        }
        try png.write(to: image, options: .atomic); try fixtureAudio(audio)
        func call(_ request: [String: Any], cancellation: LinkedCore.Cancellation = .init(),
                  event: @escaping (Data) -> Void = { _ in }) throws -> [String: Any] {
            let data = try core.execute(JSONSerialization.data(withJSONObject: request), cancellation: cancellation, event: event)
            guard let response = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw TestFailure.failed("Invalid Core response") }
            return response
        }
        func checked(_ request: [String: Any], event: @escaping (Data) -> Void = { _ in }) throws -> [String: Any] {
            let response = try call(request, event: event)
            try require(response["ok"] as? Bool == true, "Core failure: \(response)")
            return response
        }
        report["capabilities"] = try checked(["schema_version": 1, "operation": "capabilities"])
        report["probe"] = try checked(["schema_version": 1, "operation": "probe_audio", "audio": audio.path])
        var exports = [[String: Any]]()
        var progressEvents = 0
        for composition in ["fitted", "square_padded"] {
            let settings: [String: Any] = ["width": 160, "height": 90, "fps": 24,
                                           "encoding": "automatic", "composition": composition]
            let preview = directory.appendingPathComponent("\(composition).png")
            _ = try checked(["schema_version": 1, "operation": "preview", "image": image.path,
                             "output": preview.path, "settings": settings])
            try require(UIImage(contentsOfFile: preview.path) != nil, "PNG preview did not decode")
            for mode in ["simple", "per_frame"] {
                let output = directory.appendingPathComponent("\(composition)-\(mode).mp4")
                let response = try checked(["schema_version": 1, "operation": "render", "mode": mode,
                    "clips": [["id": "one", "image": image.path, "audio": audio.path, "duration_seconds": 0.5],
                              ["id": "two", "image": image.path, "audio": audio.path, "duration_seconds": 0.5]],
                    "output": output.path, "settings": settings]) { data in
                    if let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any], event["event"] as? String == "progress" { progressEvents += 1 }
                }
                exports.append(["mode": mode, "composition": composition, "core": response,
                                "decode": try await decodedCounts(output)])
            }
        }
        report["exports"] = exports
        try require(progressEvents > 0, "No progress callback received")
        report["progress_events"] = progressEvents
        let output = directory.appendingPathComponent("cancel.mp4")
        let sentinel = Data("existing destination".utf8)
        try sentinel.write(to: output)
        let cancellation = LinkedCore.Cancellation()
        let cancelled = try call(["schema_version": 1, "operation": "render", "image": image.path,
            "audio": audio.path, "output": output.path, "settings": ["width": 160, "height": 90, "encoding": "automatic"]],
            cancellation: cancellation) { data in
                if let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any], event["event"] as? String == "progress" { cancellation.cancel() }
            }
        let error = cancelled["error"] as? [String: Any]
        try require(cancelled["ok"] as? Bool == false && error?["code"] as? String == "cancelled", "Cancellation did not propagate")
        try require(try Data(contentsOf: output) == sentinel, "Cancelled export replaced destination")
        report["cancellation"] = cancelled
        report["status"] = "passed"
    } catch {
        report["status"] = "failed"
        report["error"] = String(describing: error)
    }
    return report
}

@main final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible(); self.window = window
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        Task.detached {
            let report = await runTests(directory: directory)
            if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: directory.appendingPathComponent("result.json"), options: .atomic)
                FileHandle.standardError.write(data + Data([10]))
            }
        }
        return true
    }
}
