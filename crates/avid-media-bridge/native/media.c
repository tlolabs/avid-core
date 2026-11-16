/* SPDX-License-Identifier: GPL-3.0-or-later
 * Each exported call owns all contexts. No global log callback, worker, registry,
 * network initialization, process entrypoint, or executable lookup is used.
 */
#include "media.h"
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/avutil.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libavutil/channel_layout.h>
#include <libavutil/display.h>
#include <libavutil/mathematics.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if LIBAVCODEC_VERSION_MAJOR != 63
#error AVID linked media requires its pinned FFmpeg 9 SDK (libavcodec major 63)
#endif
#define IMAGE_PIXELS INT64_C(50000000)
#define MAX_INPUTS 2048
/* Playlist demuxers and custom I/O protocols are intentionally unavailable,
 * including nested references. Only direct local media formats are accepted. */
#define LOCAL_FORMATS "mov,mp3,wav,flac,aac,aiff,ogg,matroska,webm,asf,ape,amr,ac3,eac3,dts,tta,wv,dsf,iff,au,voc,w64,caf,image2,image2pipe,jpeg_pipe,png_pipe,webp_pipe,bmp_pipe,tiff_pipe,jpegls_pipe,jpegxl_pipe,qdraw_pipe,ppm_pipe,pgm_pipe,pbm_pipe,pam_pipe,pcx_pipe,svg_pipe,ico,gif,apng"

static int failure(AvidError *e, const char *operation, int code) {
    if (e) {
        e->code = code;
        snprintf(e->operation, sizeof(e->operation), "%s", operation);
        av_strerror(code, e->detail, sizeof(e->detail));
    }
    return code;
}
static int invalid(AvidError *e, const char *operation, const char *detail) {
    failure(e, operation, AVERROR(EINVAL));
    if (e) snprintf(e->detail, sizeof(e->detail), "%s", detail);
    return AVERROR(EINVAL);
}
// Timestamp arithmetic is checked before evaluation: untrusted container clocks
// and filter timestamps must not invoke signed overflow in bridge code.
static int timestamp_add(int64_t a, int64_t b, int64_t *result) {
    if ((b > 0 && a > INT64_MAX - b) || (b < 0 && a < INT64_MIN - b)) return 0;
    *result = a + b;
    return 1;
}
static int timestamp_subtract(int64_t a, int64_t b, int64_t *result) {
    if ((b > 0 && a < INT64_MIN + b) || (b < 0 && a > INT64_MAX + b)) return 0;
    *result = a - b;
    return 1;
}
static int interrupted(void *opaque) {
    const AvidControl *control = opaque;
    return control && control->cancelled && control->cancelled(control->opaque);
}
static int check(const AvidControl *control, AvidError *e) {
    return interrupted((void *)control) ? failure(e, "interrupt linked media operation", AVERROR_EXIT) : 0;
}
static int local_path(const char *path) {
    if (!path || !*path || !strncmp(path, "//", 2) || !strncmp(path, "\\\\", 2)) return 0;
#ifndef _WIN32
    // An absolute Unix path is never interpreted as a protocol, even when
    // its native filename contains colons. Preserve native-byte path support.
    if (path[0] == '/') return 1;
#endif
    if (strstr(path, "://")) return 0;
    const char *colon = strchr(path, ':');
#ifdef _WIN32
    if (colon == path + 1 && ((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z'))) return 1;
#endif
    return !colon;
}
static int local_io_open(AVFormatContext *s, AVIOContext **pb, const char *url, int flags, AVDictionary **options) {
    if (!local_path(url)) return AVERROR(EACCES);
    AVDictionary *local_options = NULL;
    AVDictionary **effective = options ? options : &local_options;
    av_dict_set(effective, "protocol_whitelist", "file", 0);
    int ret = avio_open2(pb, url, flags, &s->interrupt_callback, effective);
    av_dict_free(&local_options);
    return ret;
}
int avid_media_open_local_input(AVFormatContext **out, const char *path, int repeat, const AvidControl *control, AvidError *e) {
    int ret;
    AVDictionary *opts = NULL;
    if (!local_path(path)) return invalid(e, "open local media", "Media must be an accessible local filesystem path; URLs, UNC paths and protocols are unsupported");
    *out = avformat_alloc_context();
    if (!*out) return failure(e, "allocate demuxer", AVERROR(ENOMEM));
    (*out)->interrupt_callback = (AVIOInterruptCB){interrupted, (void *)control};
    (*out)->io_open = local_io_open;
    (*out)->max_analyze_duration = 10 * AV_TIME_BASE;
    (*out)->probesize = 8 * 1024 * 1024;
    av_dict_set(&opts, "protocol_whitelist", "file", 0);
    av_dict_set(&opts, "format_whitelist", LOCAL_FORMATS, 0);
    av_dict_set(&opts, "ignore_loop", repeat == 3 ? "0" : "1", 0);
    av_dict_set(&opts, "pattern_type", "none", 0);
    av_dict_set(&opts, "max_pixels", "50000000", 0);
    ret = avformat_open_input(out, path, NULL, &opts);
    av_dict_free(&opts);
    if (ret < 0) return failure(e, "open local media", ret);
    unsigned streams = (*out)->nb_streams;
    if (streams > 64) return invalid(e, "inspect media streams", "Media exceeds the 64-stream resource limit");
    AVDictionary **decoder_options = av_calloc(streams ? streams : 1, sizeof(*decoder_options));
    if (!decoder_options) return failure(e, "allocate probe limits", AVERROR(ENOMEM));
    for (unsigned i = 0; i < streams; ++i) {
        av_dict_set(&decoder_options[i], "max_pixels", "50000000", 0);
        av_dict_set(&decoder_options[i], "threads", "2", 0);
    }
    ret = avformat_find_stream_info(*out, decoder_options);
    for (unsigned i = 0; i < streams; ++i) av_dict_free(&decoder_options[i]);
    av_free(decoder_options);
    if (ret < 0) return failure(e, "read media stream information", ret);
    return check(control, e);
}

const char *avid_media_version(void) { return av_version_info(); }
const char *avid_media_configuration(void) { return avcodec_configuration(); }
int avid_media_has_filter(const char *name) { return name && avfilter_get_by_name(name) != NULL; }
int avid_media_probe(const char *path, const AvidControl *control, AvidProbe *out, AvidError *e) {
    AVFormatContext *fmt = NULL;
    int ret = check(control, e);
    if (ret < 0) return ret;
    memset(out, 0, sizeof(*out));
    out->duration = NAN;
    out->audio_start = out->video_start = NAN;
    ret = avid_media_open_local_input(&fmt, path, 0, control, e);
    if (ret < 0) goto done;
    int video = av_find_best_stream(fmt, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    int audio = av_find_best_stream(fmt, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
    if (video >= 0) {
        AVStream *s = fmt->streams[video];
        out->has_video = 1;
        out->width = s->codecpar->width; out->height = s->codecpar->height;
        out->video_codec = s->codecpar->codec_id;
        out->sar_num = s->sample_aspect_ratio.num; out->sar_den = s->sample_aspect_ratio.den;
        if (s->start_time != AV_NOPTS_VALUE) out->video_start = s->start_time * av_q2d(s->time_base);
    }
    if (audio >= 0) {
        AVStream *s = fmt->streams[audio];
        out->has_audio = 1;
        out->audio_rate = s->codecpar->sample_rate;
        out->audio_channels = s->codecpar->ch_layout.nb_channels;
        out->audio_codec = s->codecpar->codec_id;
        if (s->start_time != AV_NOPTS_VALUE) out->audio_start = s->start_time * av_q2d(s->time_base);
        if (s->duration > 0 && s->duration != AV_NOPTS_VALUE) {
            double offset = s->start_time == AV_NOPTS_VALUE || fmt->start_time == AV_NOPTS_VALUE ? 0 : fmax(0, s->start_time * av_q2d(s->time_base) - (double)fmt->start_time / AV_TIME_BASE);
            out->duration = s->duration * av_q2d(s->time_base) + offset;
        }
        else if (fmt->duration > 0 && fmt->duration != AV_NOPTS_VALUE) out->duration = (double)fmt->duration / AV_TIME_BASE;
    }
    if (!out->has_video && !out->has_audio) ret = invalid(e, "inspect media", "No supported audio or image stream");
    else ret = check(control, e);
done:
    avformat_close_input(&fmt);
    return ret;
}

static int configure_video_encoder(AVCodecContext *enc, const AVCodec *codec, int width, int height, int fps, int preview, AvidError *e) {
    enc->width = width; enc->height = height;
    enc->time_base = (AVRational){1, fps}; enc->framerate = (AVRational){fps, 1};
    enc->sample_aspect_ratio = (AVRational){1, 1};
    enc->pix_fmt = preview ? AV_PIX_FMT_RGB24 : AV_PIX_FMT_YUV420P;
    enc->thread_count = 2;
    enc->bit_rate = preview ? 0 : (int64_t)width * height * fps / 8;
    enc->gop_size = fps * 2;
    if (codec->id == AV_CODEC_ID_HEVC) enc->codec_tag = MKTAG('h','v','c','1');
    AVDictionary *opts = NULL;
    if (!strcmp(codec->name, "libx264")) {
        enc->bit_rate = 0;
        av_dict_set(&opts, "crf", "23", 0);
        av_dict_set(&opts, "tune", "stillimage", 0);
        av_dict_set(&opts, "preset", "medium", 0);
    }
    if (!strcmp(codec->name, "libx265")) {
        enc->bit_rate = 0;
        av_dict_set(&opts, "crf", "28", 0);
    }
    // VideoToolbox must not internally substitute a software encoder for an
    // explicit hardware request. Automatic fallback is coordinated by Core.
    if (strstr(codec->name, "videotoolbox")) av_dict_set(&opts, "allow_sw", "0", 0);
    int ret = avcodec_open2(enc, codec, &opts);
    av_dict_free(&opts);
    if (ret < 0) {
        failure(e, "initialize video encoder", ret);
        if (e) { char reason[512]; av_strerror(ret, reason, sizeof(reason)); snprintf(e->detail, sizeof(e->detail), "Encoder %s: %s", codec->name, reason); }
    }
    return ret;
}
static int enumerate_encoders(AvidEncoderCallback callback, void *opaque, const AvidControl *control, int probe_hardware, AvidError *e) {
    void *cursor = NULL;
    const AVCodec *codec;
    while ((codec = av_codec_iterate(&cursor))) {
        int ret = check(control, e);
        if (ret < 0) return ret;
        if (!av_codec_is_encoder(codec) || (codec->id != AV_CODEC_ID_H264 && codec->id != AV_CODEC_ID_HEVC)) continue;
        int hardware = !(strstr(codec->name, "libx264") || strstr(codec->name, "libx265") || strstr(codec->name, "libopenh264"));
        int usable = 1;
        char detail[2048] = "compiled software encoder";
        if (hardware && !probe_hardware) {
            usable = 0;
            snprintf(detail, sizeof(detail), "compiled hardware encoder; initialization not probed");
        } else if (hardware) {
            AVCodecContext *enc = avcodec_alloc_context3(codec);
            if (!enc) return failure(e, "allocate encoder capability context", AVERROR(ENOMEM));
            AvidError test = {0};
            ret = configure_video_encoder(enc, codec, 128, 128, 30, 0, &test);
            usable = ret >= 0;
            snprintf(detail, sizeof(detail), "%s", usable ? "hardware encoder context initialized; full export still requires target qualification" : test.detail);
            avcodec_free_context(&enc);
        }
        callback(opaque, codec->name, codec->id == AV_CODEC_ID_H264 ? "h264" : "hevc", hardware, usable, detail);
    }
    return check(control, e);
}
int avid_media_encoders(AvidEncoderCallback callback, void *opaque, const AvidControl *control, AvidError *e) {
    return enumerate_encoders(callback, opaque, control, 1, e);
}
int avid_media_compiled_encoders(AvidEncoderCallback callback, void *opaque, const AvidControl *control, AvidError *e) {
    return enumerate_encoders(callback, opaque, control, 0, e);
}

typedef struct {
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVStream *stream;
    AVFilterContext *source, *filtered_source;
    int32_t orientation[9];
    int has_orientation;
    const AvidInput *spec;
    AVChannelLayout source_channels;
    int source_rate;
    AVPacket *packet;
    AVFrame *frame, *pending, *last;
    int index, video, repeat, fps, input_eof, decoder_eof, source_eof, pending_ready;
    int64_t next_pts, last_pts, last_duration, loop_offset;
    AVRational time_base;
} Input;
typedef struct {
    AVCodecContext *encoder;
    AVStream *stream;
    AVFilterContext *sink;
    AVFrame *frame;
    AVPacket *packet;
    int video, eof, flushed;
    int64_t next_pts;
} Output;

static void input_close_decoder(Input *in) {
    av_frame_free(&in->frame); av_frame_free(&in->pending); av_frame_free(&in->last);
    av_packet_free(&in->packet); avcodec_free_context(&in->decoder); avformat_close_input(&in->format);
}
static void input_free(Input *in) {
    input_close_decoder(in);
    av_channel_layout_uninit(&in->source_channels);
}
static void output_free(Output *out) {
    av_frame_free(&out->frame); av_packet_free(&out->packet); avcodec_free_context(&out->encoder);
}
static int valid_image_dimensions(int width, int height, AvidError *e) {
    if (width <= 0 || height <= 0 || width > 32768 || height > 32768 || (int64_t)width * height > IMAGE_PIXELS)
        return invalid(e, "validate decoded artwork dimensions", "Artwork exceeds the 32768-axis / 50 megapixel limit or has zero dimensions");
    return 0;
}
/* Returns one decoded frame, never a complete input. Each demuxed packet is
 * unreferenced before another is read. Decoders are drained at EOF. */
static int decode_next(Input *in, const AvidControl *control, AvidError *e) {
    for (;;) {
        int ret = check(control, e);
        if (ret < 0) return ret;
        ret = avcodec_receive_frame(in->decoder, in->frame);
        if (ret >= 0) {
            if (in->video && valid_image_dimensions(in->frame->width, in->frame->height, e) < 0) return AVERROR(EINVAL);
            return 1;
        }
        if (ret == AVERROR_EOF) { in->decoder_eof = 1; return 0; }
        if (ret != AVERROR(EAGAIN)) return failure(e, "decode media frame", ret);
        if (in->input_eof) return failure(e, "drain media decoder", AVERROR_INVALIDDATA);
        for (;;) {
            ret = av_read_frame(in->format, in->packet);
            if (ret < 0) {
                if (ret != AVERROR_EOF) return failure(e, "read media packet", ret);
                in->input_eof = 1;
                ret = avcodec_send_packet(in->decoder, NULL);
                if (ret < 0 && ret != AVERROR_EOF) return failure(e, "flush media decoder", ret);
                break;
            }
            if (in->packet->stream_index == in->index && (in->packet->size > 0 || in->packet->side_data_elems)) break;
            av_packet_unref(in->packet);
        }
        if (!in->input_eof) {
            ret = avcodec_send_packet(in->decoder, in->packet);
            av_packet_unref(in->packet);
            if (ret < 0) return failure(e, "submit media packet", ret);
        }
    }
}
static int input_open(Input *in, const AvidInput *spec, const AvidControl *control, AvidError *e) {
    in->spec = spec;
    in->input_eof = in->decoder_eof = 0;
    in->next_pts = in->loop_offset = in->last_duration = 0;
    int ret = avid_media_open_local_input(&in->format, spec->path, spec->repeat, control, e);
    if (ret < 0) return ret;
    in->video = spec->media == 0; in->repeat = spec->repeat; in->fps = spec->fps;
    in->index = av_find_best_stream(in->format, in->video ? AVMEDIA_TYPE_VIDEO : AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
    if (in->index < 0) return failure(e, "select media stream", in->index);
    in->stream = in->format->streams[in->index];
    if (in->video && valid_image_dimensions(in->stream->codecpar->width, in->stream->codecpar->height, e) < 0) return AVERROR(EINVAL);
    const AVCodec *codec = avcodec_find_decoder(in->stream->codecpar->codec_id);
    if (!codec) return failure(e, "find media decoder", AVERROR_DECODER_NOT_FOUND);
    in->decoder = avcodec_alloc_context3(codec);
    in->packet = av_packet_alloc(); in->frame = av_frame_alloc(); in->pending = av_frame_alloc(); in->last = av_frame_alloc();
    if (!in->decoder || !in->packet || !in->frame || !in->pending || !in->last) return failure(e, "allocate media decoder", AVERROR(ENOMEM));
    ret = avcodec_parameters_to_context(in->decoder, in->stream->codecpar);
    if (ret < 0) return failure(e, "configure media decoder", ret);
    in->decoder->thread_count = 2; in->decoder->max_pixels = IMAGE_PIXELS;
    in->decoder->pkt_timebase = in->stream->time_base;
    ret = avcodec_open2(in->decoder, codec, NULL);
    if (ret < 0) return failure(e, "initialize media decoder", ret);
    in->time_base = in->repeat == 1 ? (AVRational){1, in->fps} : in->stream->time_base;
    in->last_pts = AV_NOPTS_VALUE;
    ret = decode_next(in, control, e);
    if (ret <= 0) return ret < 0 ? ret : invalid(e, "decode first media frame", "Media stream contains no decodable frame");
    av_frame_move_ref(in->pending, in->frame); in->pending_ready = 1;
    return 0;
}
static int make_source(Input *in, AVFilterGraph *graph, size_t index, AvidError *e) {
    char args[1024], name[64], layout[256];
    AVFrame *frame = in->pending;
    int ret;
    snprintf(name, sizeof(name), "input_%zu", index);
    if (in->video) {
        AVRational sar = frame->sample_aspect_ratio;
        if (!sar.num || !sar.den) sar = (AVRational){1, 1};
        snprintf(args, sizeof(args), "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d:frame_rate=%d/1:colorspace=%d:range=%d", frame->width, frame->height, frame->format, in->time_base.num, in->time_base.den, sar.num, sar.den, in->fps, frame->colorspace, frame->color_range);
        ret = avfilter_graph_create_filter(&in->source, avfilter_get_by_name("buffer"), name, args, NULL, graph);
    } else {
        if (!frame->ch_layout.nb_channels || frame->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC) {
            int channels = frame->ch_layout.nb_channels ? frame->ch_layout.nb_channels : in->decoder->ch_layout.nb_channels;
            av_channel_layout_uninit(&frame->ch_layout);
            av_channel_layout_default(&frame->ch_layout, channels ? channels : 2);
        }
        ret = av_channel_layout_copy(&in->source_channels, &frame->ch_layout);
        if (ret < 0) return failure(e, "retain audio source layout", ret);
        in->source_rate = frame->sample_rate;
        av_channel_layout_describe(&frame->ch_layout, layout, sizeof(layout));
        snprintf(args, sizeof(args), "time_base=%d/%d:sample_rate=%d:sample_fmt=%s:channel_layout=%s", in->time_base.num, in->time_base.den, frame->sample_rate, av_get_sample_fmt_name(frame->format), layout);
        ret = avfilter_graph_create_filter(&in->source, avfilter_get_by_name("abuffer"), name, args, NULL, graph);
    }
    if (ret < 0) return failure(e, "create filter input", ret);
    in->filtered_source = in->source;
    if (in->video) {
        AVFrameSideData *side = av_frame_get_side_data(frame, AV_FRAME_DATA_DISPLAYMATRIX);
        const int32_t *matrix = side && side->size >= 9 * sizeof(int32_t) ? (const int32_t *)side->data : NULL;
        if (!matrix) {
            const AVPacketSideData *packet_side = av_packet_side_data_get(in->stream->codecpar->coded_side_data, in->stream->codecpar->nb_coded_side_data, AV_PKT_DATA_DISPLAYMATRIX);
            if (packet_side && packet_side->size >= 9 * sizeof(int32_t)) matrix = (const int32_t *)packet_side->data;
        }
        const char *filters[2] = {NULL, NULL}, *options[2] = {NULL, NULL};
        if (matrix) {
            memcpy(in->orientation, matrix, sizeof(in->orientation)); in->has_orientation = 1;
            // Apply EXIF/display transforms before user-requested flips and
            // composition. The signs account for mirrored EXIF orientations.
            double degrees = -round(av_display_rotation_get(matrix));
            if (!isfinite(degrees)) return invalid(e, "orient artwork", "Invalid display rotation matrix");
            degrees = fmod(degrees + 720.0, 360.0);
            if (fabs(degrees - 90.0) < 1.0) { filters[0] = "transpose"; options[0] = matrix[3] > 0 ? "cclock_flip" : "clock"; }
            else if (fabs(degrees - 270.0) < 1.0) { filters[0] = "transpose"; options[0] = matrix[3] < 0 ? "clock_flip" : "cclock"; }
            else if (fabs(degrees - 180.0) < 1.0) { if (matrix[0] < 0) filters[0] = "hflip"; if (matrix[4] < 0) filters[filters[0] ? 1 : 0] = "vflip"; }
            else if (degrees < 1.0 || degrees > 359.0) { if (matrix[4] < 0) filters[0] = "vflip"; }
            else return invalid(e, "orient artwork", "Arbitrary-angle display rotation requires an explicitly supported transform; no silent orientation change is allowed");
        }
        for (size_t n = 0; n < 2 && filters[n]; ++n) {
            AVFilterContext *transform = NULL;
            snprintf(name, sizeof(name), "orient_%zu_%zu", index, n);
            const AVFilter *filter = avfilter_get_by_name(filters[n]);
            if (!filter) return failure(e, "find artwork orientation filter", AVERROR_FILTER_NOT_FOUND);
            ret = avfilter_graph_create_filter(&transform, filter, name, options[n], NULL, graph);
            if (ret >= 0) ret = avfilter_link(in->filtered_source, 0, transform, 0);
            if (ret < 0) return failure(e, "configure artwork orientation", ret);
            in->filtered_source = transform;
        }
    }
    return 0;
}
/* Supply one frame only. Static artwork is held by reference; GIF rewinds decode
 * state, APNG delegates embedded loops to its demuxer and its finite tail to the
 * canonical graph. Audio and animations preserve their original time base. */
static int feed_input(Input *in, const AvidControl *control, AvidError *e) {
    int ret;
    if (in->source_eof) return 0;
    if (!in->decoder) {
        ret = input_open(in, in->spec, control, e);
        if (ret < 0) return ret;
    }
    if (in->repeat == 1 && in->last->buf[0]) {
        ret = av_frame_ref(in->frame, in->last);
        if (ret < 0) return failure(e, "reference static artwork", ret);
    } else if (in->pending_ready) {
        av_frame_move_ref(in->frame, in->pending); in->pending_ready = 0;
    } else {
        ret = decode_next(in, control, e);
        if (ret < 0) return ret;
        if (ret == 0 && in->repeat == 2) {
            // Rewind GIF without buffering a complete animation. Keep timestamps
            // increasing by the final frame duration across each rewind.
            ret = av_seek_frame(in->format, in->index, in->stream->start_time == AV_NOPTS_VALUE ? 0 : in->stream->start_time, AVSEEK_FLAG_BACKWARD);
            if (ret < 0) return failure(e, "rewind animated artwork", ret);
            avcodec_flush_buffers(in->decoder); in->input_eof = in->decoder_eof = 0;
            if (!timestamp_add(in->last_pts, in->last_duration, &in->loop_offset))
                return invalid(e, "normalize animated artwork timestamps", "Animation loop timestamp exceeds the representable range");
            ret = decode_next(in, control, e);
            if (ret < 0) return ret;
        }
        if (ret == 0) {
            in->source_eof = 1;
            ret = av_buffersrc_add_frame_flags(in->source, NULL, 0);
            input_close_decoder(in);
            return ret < 0 ? failure(e, "flush filter input", ret) : 0;
        }
    }
    if (in->repeat == 1) {
        in->frame->pts = in->next_pts;
        if (!timestamp_add(in->next_pts, 1, &in->next_pts))
            return invalid(e, "normalize static artwork timestamps", "Static artwork timestamp exceeds the representable range");
        in->frame->duration = 1;
        if (!in->last->buf[0]) {
            ret = av_frame_ref(in->last, in->frame);
            if (ret < 0) return failure(e, "hold static artwork", ret);
        }
    } else {
        int64_t pts = in->frame->best_effort_timestamp;
        if (pts == AV_NOPTS_VALUE) pts = in->next_pts;
        int64_t start = in->stream->start_time == AV_NOPTS_VALUE ? 0 : in->stream->start_time;
        // Preserve audio's delay relative to the common container origin.
        // Using its own stream start would silently advance delayed tracks.
        if (!in->video && in->format->start_time != AV_NOPTS_VALUE)
            start = av_rescale_q(in->format->start_time, AV_TIME_BASE_Q, in->time_base);
        if (!timestamp_subtract(pts, start, &pts) || !timestamp_add(pts, in->loop_offset, &pts))
            return invalid(e, "normalize input timestamps", "Media timestamp and source origin exceed the representable range");
        if (in->frame->duration > 0) in->last_duration = in->frame->duration;
        else if (in->video) in->last_duration = av_rescale_q(1, (AVRational){1, in->fps}, in->time_base);
        else in->last_duration = av_rescale_q(in->frame->nb_samples, (AVRational){1, in->frame->sample_rate}, in->time_base);
        if (in->last_duration < 1) in->last_duration = 1;
        in->frame->pts = pts; in->last_pts = pts;
        if (!timestamp_add(pts, in->last_duration, &in->next_pts))
            return invalid(e, "advance input timestamps", "Decoded frame endpoint exceeds the representable range");
    }
    if (!in->video && in->frame->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC) {
        int channels = in->frame->ch_layout.nb_channels;
        av_channel_layout_uninit(&in->frame->ch_layout);
        av_channel_layout_default(&in->frame->ch_layout, channels);
    }
    if (in->video) {
        AVFrameSideData *side = av_frame_get_side_data(in->frame, AV_FRAME_DATA_DISPLAYMATRIX);
        if (side && (side->size < sizeof(in->orientation) || !in->has_orientation || memcmp(in->orientation, side->data, sizeof(in->orientation)))) {
            av_frame_unref(in->frame);
            return invalid(e, "orient artwork", "Artwork orientation changed within one input");
        }
        av_frame_remove_side_data(in->frame, AV_FRAME_DATA_DISPLAYMATRIX);
    }
    ret = av_buffersrc_add_frame_flags(in->source, in->frame, AV_BUFFERSRC_FLAG_KEEP_REF);
    av_frame_unref(in->frame);
    if (ret < 0) return failure(e, "submit filter frame", ret);
    double consumed = in->next_pts * av_q2d(in->time_base);
    if (isfinite(in->spec->duration) && in->spec->duration > 0 && consumed + 1e-9 >= in->spec->duration) {
        in->source_eof = 1;
        ret = av_buffersrc_add_frame_flags(in->source, NULL, 0);
        input_close_decoder(in);
        if (ret < 0) return failure(e, "complete bounded clip input", ret);
    }
    return 0;
}

static int output_open(Output *out, AVFormatContext *format, const AvidPlan *plan, int video, const AVChannelLayout *channels, int sample_rate, AvidError *e) {
    out->video = video;
    const AVCodec *codec = video ? avcodec_find_encoder_by_name(plan->encoder) : avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!codec) return failure(e, "find output encoder", AVERROR_ENCODER_NOT_FOUND);
    if (video && !plan->preview && codec->id != AV_CODEC_ID_H264 && codec->id != AV_CODEC_ID_HEVC) return invalid(e, "select output encoder", "Only explicitly selected H.264/HEVC encoders are valid for MP4 export");
    out->encoder = avcodec_alloc_context3(codec); out->stream = avformat_new_stream(format, NULL);
    out->frame = av_frame_alloc(); out->packet = av_packet_alloc();
    if (!out->encoder || !out->stream || !out->frame || !out->packet) return failure(e, "allocate output encoder", AVERROR(ENOMEM));
    if (format->oformat->flags & AVFMT_GLOBALHEADER) out->encoder->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    int ret;
    if (video) ret = configure_video_encoder(out->encoder, codec, plan->width, plan->height, plan->fps, plan->preview, e);
    else {
        out->encoder->bit_rate = plan->audio_bitrate;
        int requested_rate = sample_rate > 0 ? sample_rate : 48000;
        out->encoder->sample_rate = requested_rate;
        const void *supported = NULL; int count = 0;
        ret = avcodec_get_supported_config(out->encoder, codec, AV_CODEC_CONFIG_SAMPLE_RATE, 0, &supported, &count);
        if (ret < 0) return failure(e, "inspect AAC sample-rate support", ret);
        if (supported && count > 0) {
            // Preserve a supported source rate exactly. Otherwise select the
            // nearest supported AAC rate, as automatic audio conversion did
            // in the executable backend (e.g. 192 kHz -> 96 kHz).
            const int *rates = supported;
            int selected = 0; int64_t best_distance = INT64_MAX;
            for (int n = 0; n < count; ++n) {
                if (rates[n] <= 0) continue;
                int64_t distance = llabs((int64_t)rates[n] - requested_rate);
                if (distance < best_distance || (distance == best_distance && rates[n] > selected)) {
                    selected = rates[n]; best_distance = distance;
                }
            }
            if (!selected) return invalid(e, "configure audio encoder", "AAC encoder advertises no valid sample rate");
            out->encoder->sample_rate = selected;
        }
        supported = NULL; count = 0;
        ret = avcodec_get_supported_config(out->encoder, codec, AV_CODEC_CONFIG_SAMPLE_FORMAT, 0, &supported, &count);
        if (ret < 0 || count == 0) return invalid(e, "configure audio encoder", "AAC encoder does not advertise a supported sample format");
        out->encoder->sample_fmt = ((const enum AVSampleFormat *)supported)[0];
        out->encoder->time_base = (AVRational){1, out->encoder->sample_rate}; out->encoder->thread_count = 2;
        if (channels && channels->nb_channels) ret = av_channel_layout_copy(&out->encoder->ch_layout, channels);
        else { av_channel_layout_default(&out->encoder->ch_layout, 2); ret = 0; }
        if (ret < 0) return failure(e, "configure audio channels", ret);
        supported = NULL; count = 0;
        ret = avcodec_get_supported_config(out->encoder, codec, AV_CODEC_CONFIG_CHANNEL_LAYOUT, 0, &supported, &count);
        if (ret < 0) return failure(e, "inspect AAC channel-layout support", ret);
        if (supported && count > 0) {
            const AVChannelLayout *layouts = supported;
            int compatible = 0;
            for (int n = 0; n < count; ++n)
                if (!av_channel_layout_compare(&out->encoder->ch_layout, &layouts[n])) compatible = 1;
            if (!compatible) {
                char layout[256], detail[512];
                av_channel_layout_describe(&out->encoder->ch_layout, layout, sizeof(layout));
                snprintf(detail, sizeof(detail), "AAC does not support source channel layout %s; an explicit host downmix policy is required", layout);
                return invalid(e, "configure audio channels", detail);
            }
        }
        ret = avcodec_open2(out->encoder, codec, NULL);
        if (ret < 0) return failure(e, "initialize audio encoder", ret);
    }
    if (ret < 0) return ret;
    out->stream->time_base = out->encoder->time_base;
    out->stream->sample_aspect_ratio = (AVRational){1, 1};
    ret = avcodec_parameters_from_context(out->stream->codecpar, out->encoder);
    return ret < 0 ? failure(e, "configure output stream", ret) : 0;
}
static int make_sink(Output *out, AVFilterGraph *graph, const char *name, AvidError *e) {
    out->sink = avfilter_graph_alloc_filter(graph, avfilter_get_by_name(out->video ? "buffersink" : "abuffersink"), name);
    if (!out->sink) return failure(e, "create filter output", AVERROR(ENOMEM));
    int ret = 0;
    if (out->video) {
        enum AVPixelFormat formats[] = {out->encoder->pix_fmt, AV_PIX_FMT_NONE};
        ret = av_opt_set_array(out->sink, "pixel_formats", AV_OPT_SEARCH_CHILDREN, 0, 1, AV_OPT_TYPE_PIXEL_FMT, formats);
    } else {
        enum AVSampleFormat formats[] = {out->encoder->sample_fmt, AV_SAMPLE_FMT_NONE};
        int rates[] = {out->encoder->sample_rate, -1};
        char layout[256]; av_channel_layout_describe(&out->encoder->ch_layout, layout, sizeof(layout));
        ret = av_opt_set_array(out->sink, "sample_formats", AV_OPT_SEARCH_CHILDREN, 0, 1, AV_OPT_TYPE_SAMPLE_FMT, formats);
        if (ret >= 0) ret = av_opt_set_array(out->sink, "samplerates", AV_OPT_SEARCH_CHILDREN, 0, 1, AV_OPT_TYPE_INT, rates);
        if (ret >= 0) ret = av_opt_set_array(out->sink, "channel_layouts", AV_OPT_SEARCH_CHILDREN, 0, 1, AV_OPT_TYPE_CHLAYOUT, &out->encoder->ch_layout);
    }
    if (ret >= 0) ret = avfilter_init_str(out->sink, NULL);
    return ret < 0 ? failure(e, "configure filter output", ret) : 0;
}
static int link_graph(AVFilterGraph *graph, Input *inputs, size_t count, Output *video, Output *audio, const AvidPlan *plan, AvidError *e) {
    AVFilterInOut *unconnected_inputs = NULL, *unconnected_outputs = NULL;
    AVFilterGraphSegment *segment = NULL;
    int ret = avfilter_graph_segment_parse(graph, plan->graph, 0, &segment);
    if (ret < 0) { failure(e, "parse canonical media graph", ret); goto done; }
    const char *allowed[] = {"aformat", "anull", "aresample", "asetpts", "atrim", "concat", "crop", "format", "fps", "gblur", "hflip", "loop", "null", "overlay", "pad", "scale", "setpts", "setsar", "split", "tpad", "transpose", "trim", "vflip"};
    size_t filters = 0;
    // Validate before initialization: movie/amovie/subtitles and other filters
    // that open files, protocols, fonts or services can never execute here.
    for (size_t chain = 0; chain < segment->nb_chains; ++chain) {
        AVFilterChain *current = segment->chains[chain];
        for (size_t n = 0; n < current->nb_filters; ++n) {
            const char *name = current->filters[n]->filter_name;
            int permitted = 0;
            for (size_t a = 0; a < sizeof(allowed) / sizeof(*allowed); ++a) if (name && !strcmp(name, allowed[a])) permitted = 1;
            if (!permitted || ++filters > 32768) { ret = invalid(e, "validate canonical media graph", "Graph contains an unsupported filter or exceeds the filter limit"); goto done; }
        }
    }
    ret = avfilter_graph_segment_apply(segment, 0, &unconnected_inputs, &unconnected_outputs);
    if (ret < 0) { failure(e, "initialize canonical media graph", ret); goto done; }
    for (AVFilterInOut *p = unconnected_inputs; p; p = p->next) {
        size_t index; char suffix[16] = {0}; int found = 0;
        if (sscanf(p->name, "%zu:%15s", &index, suffix) == 2 && index < count) {
            int expected_video = !strcmp(suffix, "v") || !strcmp(suffix, "v:0");
            int expected_audio = !strcmp(suffix, "a") || !strcmp(suffix, "a:0");
            if ((expected_video && inputs[index].video) || (expected_audio && !inputs[index].video)) {
                ret = avfilter_link(inputs[index].filtered_source, 0, p->filter_ctx, p->pad_idx); found = 1;
            }
        }
        if (!found) { ret = invalid(e, "link canonical media graph", "Graph references an unknown or mismatched typed input"); goto done; }
        if (ret < 0) { failure(e, "link media graph input", ret); goto done; }
    }
    int linked_video = 0, linked_audio = !audio;
    for (AVFilterInOut *p = unconnected_outputs; p; p = p->next) {
        if (!strcmp(p->name, plan->video_label)) { ret = avfilter_link(p->filter_ctx, p->pad_idx, video->sink, 0); linked_video++; }
        else if (audio && plan->audio_label && !strcmp(p->name, plan->audio_label)) { ret = avfilter_link(p->filter_ctx, p->pad_idx, audio->sink, 0); linked_audio++; }
        else { ret = invalid(e, "link canonical media graph", "Graph has an unexpected unconsumed output"); goto done; }
        if (ret < 0) { failure(e, "link media graph output", ret); goto done; }
    }
    if (linked_video != 1 || linked_audio != 1) { ret = invalid(e, "link canonical media graph", "Graph must contain exactly the requested video/audio outputs"); goto done; }
    ret = avfilter_graph_config(graph, NULL);
    if (ret < 0) failure(e, "initialize composition and resampling graph", ret);
    if (ret >= 0 && audio && audio->encoder->frame_size) av_buffersink_set_frame_size(audio->sink, audio->encoder->frame_size);
done:
    avfilter_graph_segment_free(&segment);
    avfilter_inout_free(&unconnected_inputs); avfilter_inout_free(&unconnected_outputs);
    return ret;
}
static int mux_packets(Output *out, AVFormatContext *format, const AvidControl *control, AvidError *e) {
    for (;;) {
        int ret = check(control, e);
        if (ret < 0) return ret;
        ret = avcodec_receive_packet(out->encoder, out->packet);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) return 0;
        if (ret < 0) return failure(e, "receive encoded packet", ret);
        if (out->video && out->packet->duration <= 0) out->packet->duration = 1;
        av_packet_rescale_ts(out->packet, out->encoder->time_base, out->stream->time_base);
        out->packet->stream_index = out->stream->index;
        ret = av_interleaved_write_frame(format, out->packet);
        av_packet_unref(out->packet);
        if (ret < 0) return failure(e, "mux output packet", ret);
    }
}
static int encode_frame(Output *out, AVFormatContext *format, AVFrame *frame, const AvidControl *control, AvidError *e) {
    int ret = avcodec_send_frame(out->encoder, frame);
    if (ret < 0) return failure(e, frame ? "encode media frame" : "flush media encoder", ret);
    return mux_packets(out, format, control, e);
}
int avid_media_execute(const AvidPlan *plan, const AvidControl *control, AvidError *e) {
    int ret = check(control, e), header_written = 0;
    Input *inputs = NULL;
    AVFilterGraph *graph = NULL;
    AVFormatContext *format = NULL;
    Output video = {0}, audio = {0};
    if (ret < 0) return ret;
    if (!plan || !plan->input_count || plan->input_count > MAX_INPUTS || plan->fps < 1 || plan->fps > 240 || !local_path(plan->output)) return invalid(e, "validate linked execution plan", "Invalid input count, frame rate, or local output path");
    if (plan->width < 1 || plan->height < 1 || plan->width > 8192 || plan->height > 8192 || (int64_t)plan->width * plan->height > 33177600) return invalid(e, "validate linked execution plan", "Invalid output dimensions");
    inputs = av_calloc(plan->input_count, sizeof(*inputs));
    graph = avfilter_graph_alloc();
    if (!inputs || !graph) { ret = failure(e, "allocate media graph", AVERROR(ENOMEM)); goto done; }
    graph->nb_threads = 1;
    for (size_t i = 0; i < plan->input_count; ++i) {
        ret = input_open(&inputs[i], &plan->inputs[i], control, e);
        if (ret < 0) goto done;
        ret = make_source(&inputs[i], graph, i, e);
        if (ret < 0) goto done;
        // Graph setup retains only format/layout metadata. Dormant timeline
        // clips own no decoder threads, compressed packets, or decoded frames.
        input_close_decoder(&inputs[i]);
    }
    ret = avformat_alloc_output_context2(&format, NULL, plan->preview ? "image2" : "mp4", plan->output);
    if (ret < 0 || !format) { ret = failure(e, "allocate media muxer", ret < 0 ? ret : AVERROR(ENOMEM)); goto done; }
    format->interrupt_callback = (AVIOInterruptCB){interrupted, (void *)control}; format->io_open = local_io_open;
    ret = output_open(&video, format, plan, 1, NULL, 0, e);
    if (ret < 0) goto done;
    if (!plan->preview) {
        const AVChannelLayout *channels = NULL;
        // The canonical timeline graph normalizes to stereo. Single-source
        // exports preserve the decoded input channel layout.
        int sample_rate = 48000;
        size_t audio_inputs = 0;
        for (size_t i = 0; i < plan->input_count; ++i) if (!inputs[i].video) { audio_inputs++; channels = &inputs[i].source_channels; sample_rate = inputs[i].source_rate; }
        if (audio_inputs != 1 || plan->normalize_audio) { channels = NULL; sample_rate = 48000; }
        ret = output_open(&audio, format, plan, 0, channels, sample_rate, e);
        if (ret < 0) goto done;
    }
    ret = make_sink(&video, graph, "video_output", e);
    if (ret < 0) goto done;
    if (!plan->preview && (ret = make_sink(&audio, graph, "audio_output", e)) < 0) goto done;
    ret = link_graph(graph, inputs, plan->input_count, &video, plan->preview ? NULL : &audio, plan, e);
    if (ret < 0) goto done;
    AVDictionary *io_opts = NULL;
    av_dict_set(&io_opts, "protocol_whitelist", "file", 0);
    if (!(format->oformat->flags & AVFMT_NOFILE)) ret = avio_open2(&format->pb, plan->output, AVIO_FLAG_WRITE, &format->interrupt_callback, &io_opts);
    av_dict_free(&io_opts);
    if (ret < 0) { failure(e, "open staged media output", ret); goto done; }
    AVDictionary *mux_opts = NULL;
    if (!plan->preview) av_dict_set(&mux_opts, "movflags", "+faststart", 0);
    else av_dict_set(&mux_opts, "update", "1", 0);
    ret = avformat_write_header(format, &mux_opts);
    av_dict_free(&mux_opts);
    if (ret < 0) { failure(e, "write media header", ret); goto done; }
    header_written = 1;
    int64_t video_frames = 0;
    double end = isfinite(plan->duration) && plan->duration > 0 ? plan->duration : INFINITY;
    size_t round_robin = 0;
    while (!video.eof || (!plan->preview && !audio.eof)) {
        ret = check(control, e);
        if (ret < 0) goto done;
        Output *out = &video;
        if (!plan->preview && !audio.eof && (video.eof || av_compare_ts(audio.next_pts, audio.encoder->time_base, video.next_pts, video.encoder->time_base) <= 0)) out = &audio;
        ret = av_buffersink_get_frame(out->sink, out->frame);
        if (ret == AVERROR(EAGAIN) && !plan->preview) {
            // A concat audio output may need its corresponding video branch
            // drained before it can pad a short track and advance segments.
            // Poll the other live sink before supplying any more input, so a
            // lagging audio clock cannot make queued video grow with duration.
            Output *other = out == &audio ? &video : &audio;
            if (!other->eof) {
                int other_ret = av_buffersink_get_frame(other->sink, other->frame);
                if (other_ret != AVERROR(EAGAIN)) { out = other; ret = other_ret; }
            }
        }
        if (ret == AVERROR(EAGAIN)) {
            // Supply only an input actually requested by the graph. Never
            // eagerly open/drain a future timeline segment to make progress.
            unsigned most = 0; size_t chosen = SIZE_MAX;
            for (size_t n = 0; n < plan->input_count; ++n) {
                size_t i = (round_robin + n) % plan->input_count;
                if (inputs[i].source_eof) continue;
                unsigned requests = av_buffersrc_get_nb_failed_requests(inputs[i].source);
                if (requests > most) { most = requests; chosen = i; }
            }
            if (chosen == SIZE_MAX) { ret = invalid(e, "drain composition graph", "Filter outputs requested no live input and produced no frame"); goto done; }
            ret = feed_input(&inputs[chosen], control, e);
            if (ret < 0) goto done;
            round_robin = (chosen + 1) % plan->input_count;
            continue;
        }
        if (ret == AVERROR_EOF) { out->eof = 1; if (out == &audio && (!isfinite(plan->duration) || plan->duration <= 0)) end = audio.next_pts * av_q2d(audio.encoder->time_base); continue; }
        if (ret < 0) { failure(e, "receive composed media frame", ret); goto done; }
        AVRational filter_tb = av_buffersink_get_time_base(out->sink);
        if (out->video) {
            // Canonical graphs preserve animation PTS; encode CFR by selecting
            // and duplicating frames on the requested output cadence.
            int64_t target = out->frame->pts == AV_NOPTS_VALUE ? video_frames : av_rescale_q_rnd(out->frame->pts, filter_tb, out->encoder->time_base, AV_ROUND_NEAR_INF | AV_ROUND_PASS_MINMAX);
            if (target < 0) { ret = invalid(e, "normalize video timestamps", "Video timestamp is negative or outside the representable output range"); goto done; }
            if (target < video_frames) { av_frame_unref(out->frame); continue; }
            // Both values are nonnegative here, so subtraction is safe. Bound
            // the difference before adding one (INT64_MAX is a valid FFmpeg
            // timestamp sentinel and must not wrap the repeat count).
            int64_t difference = target - video_frames;
            if (difference >= INT64_C(240) * 86400) { ret = invalid(e, "normalize video timestamps", "Artwork timestamp discontinuity exceeds one day"); goto done; }
            int64_t repeats = difference + 1;
            while (repeats-- > 0) {
                if ((double)video_frames / plan->fps >= end || (plan->preview && video_frames >= 1)) { out->eof = 1; break; }
                out->frame->pts = video_frames;
                if (!timestamp_add(video_frames, 1, &video_frames)) { ret = invalid(e, "advance video timestamps", "Video frame endpoint exceeds the representable range"); goto done; }
                out->frame->duration = 1;
                out->frame->pict_type = AV_PICTURE_TYPE_NONE;
                ret = encode_frame(out, format, out->frame, control, e);
                if (ret < 0) goto done;
            }
            out->next_pts = video_frames;
            if (plan->preview && video_frames >= 1) out->eof = 1;
            if (control && control->progress) control->progress(control->opaque, (double)video_frames / plan->fps);
        } else {
            out->frame->pts = av_rescale_q(out->frame->pts, filter_tb, out->encoder->time_base);
            if (out->frame->pts == AV_NOPTS_VALUE) out->frame->pts = out->next_pts;
            int64_t end_sample = INT64_MAX;
            if (isfinite(end)) {
                long double samples = (long double)end * out->encoder->sample_rate;
                if (samples >= (long double)INT64_MAX || samples < 0.5L) {
                    ret = invalid(e, "bound output audio duration", "Duration is outside the representable output audio sample range"); goto done;
                }
                end_sample = (int64_t)llroundl(samples);
            }
            // Compare and trim in one integer sample domain. Mixing a floating
            // time comparison with a rounded limit can otherwise submit a
            // zero-sample frame at fractional sample endpoints.
            if (out->frame->pts >= end_sample) out->eof = 1;
            else {
                if (isfinite(end)) {
                    int64_t remaining;
                    if (!timestamp_subtract(end_sample, out->frame->pts, &remaining)) { ret = invalid(e, "normalize audio timestamps", "Audio timestamp is outside the representable output span"); goto done; }
                    if (remaining <= out->frame->nb_samples) {
                        out->frame->nb_samples = (int)remaining;
                        // AAC accepts a short frame only as its final frame.
                        // Stop this sink immediately; normal encoder flush
                        // below still drains every buffered packet.
                        out->eof = 1;
                    }
                }
                if (out->frame->nb_samples <= 0) { ret = invalid(e, "encode audio frame", "Decoded audio frame contains no samples"); goto done; }
                if (!timestamp_add(out->frame->pts, out->frame->nb_samples, &out->next_pts)) { ret = invalid(e, "advance audio timestamps", "Audio frame endpoint exceeds the representable range"); goto done; }
                ret = encode_frame(out, format, out->frame, control, e);
                if (ret < 0) goto done;
            }
        }
        av_frame_unref(out->frame);
    }
    ret = check(control, e);
    if (ret < 0) goto done;
    ret = encode_frame(&video, format, NULL, control, e);
    if (ret < 0) goto done;
    if (!plan->preview && (ret = encode_frame(&audio, format, NULL, control, e)) < 0) goto done;
    ret = av_write_trailer(format);
    if (ret < 0) { failure(e, "finalize media container", ret); goto done; }
    if (format->pb && format->pb->error < 0) { ret = failure(e, "flush staged output bytes", format->pb->error); goto done; }
    ret = check(control, e);
done:
    (void)header_written;
    avfilter_graph_free(&graph);
    if (inputs) for (size_t i = 0; i < plan->input_count; ++i) input_free(&inputs[i]);
    av_free(inputs);
    output_free(&video); output_free(&audio);
    if (format) {
        if (!(format->oformat->flags & AVFMT_NOFILE) && format->pb) {
            int close_ret = avio_closep(&format->pb);
            if (ret >= 0 && close_ret < 0) ret = failure(e, "close staged output bytes", close_ret);
        }
        avformat_free_context(format);
    }
    return ret;
}

int avid_media_decode_video(const char *path, const AvidControl *control, AvidVideoCallback callback, void *opaque, AvidError *e) {
    Input input = {0};
    AvidInput spec = {path, 0, 0, 30, NAN};
    struct SwsContext *scaler = NULL;
    AVFrame *rgb = NULL;
    int ret = input_open(&input, &spec, control, e);
    if (ret < 0) goto done;
    rgb = av_frame_alloc();
    if (!rgb) { ret = failure(e, "allocate RGB decode frame", AVERROR(ENOMEM)); goto done; }
    rgb->format = AV_PIX_FMT_RGB24;
    rgb->width = input.pending->width; rgb->height = input.pending->height;
    ret = av_frame_get_buffer(rgb, 1);
    if (ret < 0) { failure(e, "allocate RGB decode pixels", ret); goto done; }
    for (;;) {
        if ((ret = check(control, e)) < 0) goto done;
        if (input.pending_ready) { av_frame_move_ref(input.frame, input.pending); input.pending_ready = 0; ret = 1; }
        else ret = decode_next(&input, control, e);
        if (ret < 0) goto done;
        if (!ret) break;
        if (input.frame->width != rgb->width || input.frame->height != rgb->height) { ret = invalid(e, "decode RGB video", "Mid-stream dimension changes are unsupported"); goto done; }
        scaler = sws_getCachedContext(scaler, input.frame->width, input.frame->height, input.frame->format, rgb->width, rgb->height, AV_PIX_FMT_RGB24, SWS_BILINEAR, NULL, NULL, NULL);
        if (!scaler) { ret = failure(e, "initialize RGB conversion", AVERROR(ENOMEM)); goto done; }
        ret = av_frame_make_writable(rgb);
        if (ret < 0) { failure(e, "make RGB buffer writable", ret); goto done; }
        ret = sws_scale(scaler, (const uint8_t *const *)input.frame->data, input.frame->linesize, 0, input.frame->height, rgb->data, rgb->linesize);
        if (ret != rgb->height) { ret = invalid(e, "convert decoded video pixels", "Incomplete RGB conversion"); goto done; }
        int64_t pts = input.frame->best_effort_timestamp;
        double seconds = pts == AV_NOPTS_VALUE ? NAN : pts * av_q2d(input.stream->time_base);
        int keep_going = callback(opaque, rgb->data[0], (size_t)rgb->linesize[0] * rgb->height, rgb->linesize[0], rgb->width, rgb->height, seconds);
        av_frame_unref(input.frame);
        if (!keep_going) break;
    }
    ret = check(control, e);
done:
    sws_freeContext(scaler); av_frame_free(&rgb); input_free(&input);
    return ret;
}
