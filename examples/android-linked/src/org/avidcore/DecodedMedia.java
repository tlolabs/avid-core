package org.avidcore;

import android.graphics.Rect;
import android.media.AudioFormat;
import android.media.Image;
import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaExtractor;
import android.media.MediaFormat;
import android.os.SystemClock;
import org.json.JSONObject;
import java.io.File;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/** Independent Android decoder assertions for the two-second test fixture.
 * Keeps only one codec input/output buffer at a time; does not render media. */
final class DecodedMedia {
    private static void require(boolean condition, String detail) {
        if (!condition) throw new IllegalStateException(detail);
    }

    static JSONObject inspect(File file, boolean timeline) throws Exception {
        int rate = timeline ? 48000 : 44100, channels = timeline ? 2 : 1;
        return new JSONObject().put("video", decode(file, true, rate, channels)).put("audio", decode(file, false, rate, channels));
    }

    private static int component(Image image, int plane, int x, int y) {
        Image.Plane data = image.getPlanes()[plane];
        ByteBuffer buffer = data.getBuffer();
        return buffer.get(buffer.position() + y * data.getRowStride() + x * data.getPixelStride()) & 255;
    }

    private static JSONObject decode(File file, boolean video, int rate, int channels) throws Exception {
        MediaExtractor extractor = new MediaExtractor();
        MediaCodec decoder = null;
        boolean started = false;
        try {
            extractor.setDataSource(file.getPath());
            MediaFormat format = null;
            for (int i = 0; i < extractor.getTrackCount(); i++) {
                MediaFormat candidate = extractor.getTrackFormat(i);
                String mime = candidate.getString(MediaFormat.KEY_MIME);
                if (mime != null && mime.startsWith(video ? "video/" : "audio/")) {
                    require(format == null, "Unexpected duplicate track");
                    format = candidate;
                    extractor.selectTrack(i);
                }
            }
            require(format != null, "Missing " + (video ? "video" : "audio") + " track");
            String mime = format.getString(MediaFormat.KEY_MIME);
            require((video ? "video/avc" : "audio/mp4a-latm").equals(mime), "Unexpected codec " + mime);
            if (video) {
                require(format.getInteger(MediaFormat.KEY_WIDTH) == 160 && format.getInteger(MediaFormat.KEY_HEIGHT) == 90, "MP4 geometry mismatch");
                format.setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible);
            } else {
                require(format.getInteger(MediaFormat.KEY_SAMPLE_RATE) == rate && format.getInteger(MediaFormat.KEY_CHANNEL_COUNT) == channels, "MP4 audio layout mismatch");
                format.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_16BIT);
            }
            decoder = MediaCodec.createDecoderByType(mime);
            decoder.configure(format, null, null, 0);
            decoder.start(); started = true;
            MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();
            boolean inputEnded = false, outputEnded = false;
            int frames = 0, audioSamples = 0, crossings = 0, previousSample = 0;
            long magnitude = 0, firstPts = Long.MIN_VALUE, lastPts = Long.MIN_VALUE;
            long deadline = SystemClock.elapsedRealtime() + 30000;
            while (!outputEnded) {
                require(SystemClock.elapsedRealtime() < deadline, "Independent decoder timed out");
                if (!inputEnded) {
                    int index = decoder.dequeueInputBuffer(1000);
                    if (index >= 0) {
                        ByteBuffer input = decoder.getInputBuffer(index);
                        require(input != null, "Missing decoder input buffer");
                        input.clear();
                        int length = extractor.readSampleData(input, 0);
                        if (length < 0) {
                            inputEnded = true;
                            decoder.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM);
                        } else {
                            decoder.queueInputBuffer(index, 0, length, extractor.getSampleTime(), 0);
                            extractor.advance();
                        }
                    }
                }
                int index = decoder.dequeueOutputBuffer(info, 1000);
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    MediaFormat decoded = decoder.getOutputFormat();
                    if (!video) {
                        require(decoded.getInteger(MediaFormat.KEY_SAMPLE_RATE) == rate && decoded.getInteger(MediaFormat.KEY_CHANNEL_COUNT) == channels, "Decoded audio layout changed");
                        require(!decoded.containsKey(MediaFormat.KEY_PCM_ENCODING) || decoded.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_16BIT, "Decoder did not produce PCM16");
                    }
                } else if (index >= 0) {
                    try {
                        if (info.size > 0 && (info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) == 0) {
                            require(lastPts == Long.MIN_VALUE || info.presentationTimeUs >= lastPts, "Decoded timestamps went backwards");
                            if (firstPts == Long.MIN_VALUE) firstPts = info.presentationTimeUs;
                            lastPts = info.presentationTimeUs;
                            if (video) {
                                try (Image image = decoder.getOutputImage(index)) {
                                    require(image != null, "Video did not decode to pixels");
                                    Rect crop = image.getCropRect();
                                    require(crop.width() == 160 && crop.height() == 90 && image.getPlanes().length == 3, "Decoded geometry/planes mismatch");
                                    require(image.getPlanes()[0].getBuffer().hasRemaining(), "Missing decoded luma");
                                    int x = crop.centerX(), y = crop.centerY();
                                    int luma = component(image, 0, x, y), u = component(image, 1, x / 2, y / 2), v = component(image, 2, x / 2, y / 2);
                                    require(luma > 30 && luma < 130 && u < 150 && v > 170, "Decoded artwork is not red");
                                }
                                frames++;
                            } else {
                                ByteBuffer output = decoder.getOutputBuffer(index);
                                require(output != null && (info.size & 1) == 0, "Invalid PCM buffer");
                                ByteBuffer pcm = output.duplicate().order(ByteOrder.LITTLE_ENDIAN);
                                pcm.position(info.offset); pcm.limit(info.offset + info.size);
                                while (pcm.hasRemaining()) {
                                    int sample = pcm.getShort();
                                    if (audioSamples > 0 && (sample < 0) != (previousSample < 0)) crossings++;
                                    magnitude += Math.abs(sample);
                                    previousSample = sample; audioSamples++;
                                }
                            }
                        }
                        outputEnded = (info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
                    } finally { decoder.releaseOutputBuffer(index, false); }
                }
            }
            if (video) {
                // One frame / AAC packet tolerance accounts for encoder endpoints.
                require(frames >= 47 && frames <= 49, "Decoded video frame count: " + frames);
                require(Math.abs(firstPts) <= 50000 && lastPts >= 1900000 && lastPts <= 2050000, "Decoded video timing mismatch");
                return new JSONObject().put("decoded_frames", frames).put("first_pts_us", firstPts).put("last_pts_us", lastPts);
            }
            require(audioSamples % channels == 0 && Math.abs(audioSamples / channels - rate * 2) <= 2048, "Decoded audio sample count: " + audioSamples);
            require(magnitude / Math.max(1, audioSamples) > 1000 && crossings >= 1400 && crossings <= 2100, "Decoded tone is missing or incorrect");
            require(Math.abs(firstPts) <= 50000 && lastPts >= 1900000 && lastPts <= 2050000, "Decoded audio timing mismatch");
            return new JSONObject().put("decoded_sample_frames", audioSamples / channels).put("sample_rate", rate).put("channels", channels).put("zero_crossings", crossings).put("mean_absolute_sample", magnitude / audioSamples).put("first_pts_us", firstPts).put("last_pts_us", lastPts);
        } finally {
            if (decoder != null) {
                try { if (started) decoder.stop(); } finally { decoder.release(); }
            }
            extractor.release();
        }
    }
}
