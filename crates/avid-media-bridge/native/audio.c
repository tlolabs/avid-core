/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "audio.h"
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libswresample/swresample.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    const AvidControl *control;
    AvidAudioCallback callback;
    void *opaque;
    SwrContext *swr;
    int rate, channels;
    int input_rate;
    enum AVSampleFormat input_format;
    AVChannelLayout input_layout;
    int64_t emitted;
    double expected_input, duration;
    int have_input;
    float *samples;
    int capacity;
    AvidError *error;
} AudioDecode;

static int audio_error(AudioDecode *s, const char *operation, int code, const char *detail) {
    if (s->error) {
        s->error->code = code;
        snprintf(s->error->operation, sizeof(s->error->operation), "%s", operation);
        if (detail) snprintf(s->error->detail, sizeof(s->error->detail), "%s", detail);
        else av_strerror(code, s->error->detail, sizeof(s->error->detail));
    }
    return code;
}
static int audio_cancelled(void *opaque) {
    AudioDecode *s = opaque;
    return s->control && s->control->cancelled && s->control->cancelled(s->control->opaque);
}
static int audio_emit(AudioDecode *s, const float *samples, int count) {
    if (!count) return 0;
    if (audio_cancelled(s)) return audio_error(s, "decode audio", AVERROR_EXIT, "operation interrupted");
    if (s->callback(s->opaque, samples, (size_t)count, s->channels, (double)s->emitted / s->rate))
        return audio_error(s, "consume audio", AVERROR_EXIT, "audio consumer stopped");
    s->emitted += count;
    if (s->control && s->control->progress)
        s->control->progress(s->control->opaque, (double)s->emitted / s->rate);
    return 0;
}
static int audio_reserve(AudioDecode *s, int count) {
    if (count < 0 || count > 1048576) return audio_error(s, "resample audio", AVERROR(EINVAL), "audio frame exceeds bounded decode limit");
    if (count > s->capacity) {
        float *next = av_realloc_array(s->samples, (size_t)count * s->channels, sizeof(float));
        if (!next) return audio_error(s, "allocate audio frame", AVERROR(ENOMEM), NULL);
        s->samples = next;
        s->capacity = count;
    }
    return 0;
}
static int audio_frame(AudioDecode *s, AVFrame *frame, AVRational time_base) {
    if (frame->sample_rate <= 0 || frame->nb_samples < 0 || frame->nb_samples > 1048576)
        return audio_error(s, "decode audio", AVERROR_INVALIDDATA, "invalid decoded audio frame size/rate");
    // swr_convert interprets extended_data using the configuration fixed at
    // initialization. Reject changed frame parameters before it can read data
    // with a stale sample format, channel count or sample-rate assumption.
    if (frame->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC && frame->ch_layout.nb_channels > 0) {
        int channels = frame->ch_layout.nb_channels;
        av_channel_layout_uninit(&frame->ch_layout);
        av_channel_layout_default(&frame->ch_layout, channels);
    }
    if (frame->sample_rate != s->input_rate || frame->format != s->input_format ||
        av_channel_layout_compare(&frame->ch_layout, &s->input_layout) != 0)
        return audio_error(s, "validate decoded audio parameters", AVERROR_INVALIDDATA,
                           "mid-stream audio sample rate, format or channel layout changed");
    /* Some audio decoders (for example WMA's final drained frame) return more
     * samples without another timestamp. Once a real frame anchors the clock,
     * its sample count determines the next frame's start exactly. Do not invent
     * an initial origin, and continue validating every timestamp that is present. */
    if (frame->best_effort_timestamp == AV_NOPTS_VALUE && !s->have_input)
        return audio_error(s, "decode audio timestamps", AVERROR_INVALIDDATA, "initial audio presentation timestamp is missing");
    double pts = frame->best_effort_timestamp == AV_NOPTS_VALUE
                     ? s->expected_input
                     : frame->best_effort_timestamp * av_q2d(time_base);
    if (!isfinite(pts)) return audio_error(s, "decode audio timestamps", AVERROR_INVALIDDATA, "invalid audio timestamp");
    if (!s->have_input) {
        if (pts < -0.001) return audio_error(s, "decode audio timestamps", AVERROR_INVALIDDATA, "negative initial audio timestamp");
        /* Preserve edit-list/track-delay initial silence instead of moving speech to zero. */
        if (pts > 86400)
            return audio_error(s, "decode audio timestamps", AVERROR_INVALIDDATA, "initial audio silence exceeds one day");
        int64_t gap = (int64_t)llround(fmax(pts, 0) * s->rate);
        int result = audio_reserve(s, 16384);
        if (result < 0) return result;
        memset(s->samples, 0, (size_t)16384 * s->channels * sizeof(float));
        while (gap > 0) {
            int count = (int)(gap > 16384 ? 16384 : gap);
            result = audio_emit(s, s->samples, count);
            if (result < 0) return result;
            gap -= count;
        }
        s->have_input = 1;
    } else if (fabs(pts - s->expected_input) > 0.001) {
        return audio_error(s, "decode audio timestamps", AVERROR_INVALIDDATA, "audio presentation timestamps are discontinuous");
    }
    s->expected_input = pts + (double)frame->nb_samples / frame->sample_rate;
    int count = (int)av_rescale_rnd(swr_get_delay(s->swr, frame->sample_rate) + frame->nb_samples,
                                  s->rate, frame->sample_rate, AV_ROUND_UP);
    int result = audio_reserve(s, count);
    if (result < 0) return result;
    uint8_t *out = (uint8_t *)s->samples;
    count = swr_convert(s->swr, &out, count, (const uint8_t **)frame->extended_data, frame->nb_samples);
    if (count < 0) return audio_error(s, "resample audio", count, NULL);
    return audio_emit(s, s->samples, count);
}
static int audio_receive(AudioDecode *s, AVCodecContext *decoder, AVFrame *frame, AVRational time_base) {
    int result;
    while ((result = avcodec_receive_frame(decoder, frame)) >= 0) {
        result = audio_frame(s, frame, time_base);
        av_frame_unref(frame);
        if (result < 0) return result;
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : audio_error(s, "decode audio", result, NULL);
}
int avid_media_decode_audio(const char *path, int rate, int channels, AvidAudioCallback callback,
                            void *opaque, const AvidControl *control, AvidError *error) {
    AudioDecode s = {.control = control, .callback = callback, .opaque = opaque,
                     .rate = rate, .channels = channels, .error = error};
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    AVPacket *packet = NULL;
    AVFrame *frame = NULL;
    AVChannelLayout layout = {0};
    int result = 0, index;
    if (!path || !callback || rate < 8000 || rate > 192000 || channels < 1 || channels > 2)
        return audio_error(&s, "open audio", AVERROR(EINVAL), "invalid audio decode request");
    result = avid_media_open_local_input(&format, path, 0, control, error);
    if (result < 0) goto cleanup;
    const AVCodec *codec = NULL;
    index = av_find_best_stream(format, AVMEDIA_TYPE_AUDIO, -1, -1, &codec, 0);
    if (index < 0) { result = audio_error(&s, "find audio track", index, NULL); goto cleanup; }
    decoder = avcodec_alloc_context3(codec);
    if (!decoder) { result = audio_error(&s, "allocate audio decoder", AVERROR(ENOMEM), NULL); goto cleanup; }
    result = avcodec_parameters_to_context(decoder, format->streams[index]->codecpar);
    if (result < 0) { audio_error(&s, "configure audio decoder", result, NULL); goto cleanup; }
    /* libavcodec consumes skip/discard side data itself. It needs the packet
     * time base to move PTS/DTS when priming removes only part of a frame;
     * otherwise valid AAC/Opus samples keep their pre-trim negative PTS. */
    decoder->pkt_timebase = format->streams[index]->time_base;
    decoder->thread_count = 1;
    result = avcodec_open2(decoder, codec, NULL);
    if (result < 0) { audio_error(&s, "open audio decoder", result, NULL); goto cleanup; }
    if (decoder->sample_rate <= 0 || decoder->ch_layout.nb_channels <= 0) {
        result = audio_error(&s, "configure audio resampler", AVERROR_INVALIDDATA, "audio rate/channel layout unavailable"); goto cleanup;
    }
    if (decoder->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC) {
        int input_channels = decoder->ch_layout.nb_channels;
        av_channel_layout_uninit(&decoder->ch_layout);
        av_channel_layout_default(&decoder->ch_layout, input_channels);
    }
    s.input_rate = decoder->sample_rate;
    s.input_format = decoder->sample_fmt;
    result = av_channel_layout_copy(&s.input_layout, &decoder->ch_layout);
    if (result < 0) { audio_error(&s, "retain input audio layout", result, NULL); goto cleanup; }
    av_channel_layout_default(&layout, channels);
    result = swr_alloc_set_opts2(&s.swr, &layout, AV_SAMPLE_FMT_FLT, rate, &decoder->ch_layout,
                                 decoder->sample_fmt, decoder->sample_rate, 0, NULL);
    if (result < 0 || !s.swr) { result = audio_error(&s, "allocate audio resampler", result < 0 ? result : AVERROR(ENOMEM), NULL); goto cleanup; }
    result = swr_init(s.swr);
    if (result < 0) { audio_error(&s, "open audio resampler", result, NULL); goto cleanup; }
    s.duration = format->duration == AV_NOPTS_VALUE ? 0 : (double)format->duration / AV_TIME_BASE;
    packet = av_packet_alloc();
    frame = av_frame_alloc();
    if (!packet || !frame) { result = audio_error(&s, "allocate audio buffers", AVERROR(ENOMEM), NULL); goto cleanup; }
    while (!audio_cancelled(&s)) {
        result = av_read_frame(format, packet);
        if (result == AVERROR_EOF) break;
        if (result < 0) { audio_error(&s, "read audio", result, NULL); goto cleanup; }
        if (packet->stream_index == index) {
            result = avcodec_send_packet(decoder, packet);
            if (result < 0) { audio_error(&s, "submit audio packet", result, NULL); goto cleanup; }
            result = audio_receive(&s, decoder, frame, format->streams[index]->time_base);
            if (result < 0) goto cleanup;
        }
        av_packet_unref(packet);
    }
    if (audio_cancelled(&s)) { result = audio_error(&s, "decode audio", AVERROR_EXIT, "operation interrupted"); goto cleanup; }
    result = avcodec_send_packet(decoder, NULL);
    if (result < 0 && result != AVERROR_EOF) { audio_error(&s, "flush audio decoder", result, NULL); goto cleanup; }
    result = audio_receive(&s, decoder, frame, format->streams[index]->time_base);
    if (result < 0) goto cleanup;
    while (1) {
        result = audio_reserve(&s, 16384);
        if (result < 0) goto cleanup;
        uint8_t *out = (uint8_t *)s.samples;
        int count = swr_convert(s.swr, &out, 16384, NULL, 0);
        if (count < 0) { result = audio_error(&s, "flush audio resampler", count, NULL); goto cleanup; }
        if (!count) break;
        result = audio_emit(&s, s.samples, count);
        if (result < 0) goto cleanup;
    }
    if (!s.emitted) result = audio_error(&s, "decode audio", AVERROR_INVALIDDATA, "audio track contains no samples");
    /* A host can cancel in the final progress callback delivered while draining
     * libswresample. No next demux/decoder iteration exists to observe it. */
    else if (audio_cancelled(&s)) result = audio_error(&s, "decode audio", AVERROR_EXIT, "operation interrupted");
cleanup:
    av_channel_layout_uninit(&s.input_layout);
    av_channel_layout_uninit(&layout);
    av_free(s.samples);
    swr_free(&s.swr);
    av_frame_free(&frame);
    av_packet_free(&packet);
    avcodec_free_context(&decoder);
    avformat_close_input(&format);
    return result;
}
