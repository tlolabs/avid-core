/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef AVID_LINKED_MEDIA_H
#define AVID_LINKED_MEDIA_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    void *opaque;
    int (*cancelled)(void *);
    void (*progress)(void *, double);
} AvidControl;
typedef struct { int code; char operation[96]; char detail[2048]; } AvidError;
typedef struct {
    int width, height, has_audio, has_video;
    double duration;
    int audio_rate, audio_channels;
    int video_codec, audio_codec;
    double video_start, audio_start;
    int sar_num, sar_den;
} AvidProbe;
typedef struct { const char *path; int media, repeat, fps; double duration; } AvidInput;
typedef struct {
    const AvidInput *inputs;
    size_t input_count;
    const char *graph, *video_label, *audio_label, *output, *encoder;
    int width, height, fps, preview, normalize_audio;
    int64_t audio_bitrate;
    double duration;
} AvidPlan;
typedef void (*AvidEncoderCallback)(void *, const char *, const char *, int, int, const char *);
struct AVFormatContext;
int avid_media_open_local_input(struct AVFormatContext **, const char *, int, const AvidControl *, AvidError *);
int avid_media_probe(const char *, const AvidControl *, AvidProbe *, AvidError *);
int avid_media_execute(const AvidPlan *, const AvidControl *, AvidError *);
int avid_media_encoders(AvidEncoderCallback, void *, const AvidControl *, AvidError *);
int avid_media_compiled_encoders(AvidEncoderCallback, void *, const AvidControl *, AvidError *);
int avid_media_has_filter(const char *);
const char *avid_media_version(void);
const char *avid_media_configuration(void);
/* Diagnostic/host video decoding is streamed as borrowed RGB24 rows. Returning
 * zero from the callback ends successfully; cancellation remains Control-owned. */
typedef int (*AvidVideoCallback)(void *, const uint8_t *, size_t, int, int, int, double);
int avid_media_decode_video(const char *, const AvidControl *, AvidVideoCallback, void *, AvidError *);

#endif
