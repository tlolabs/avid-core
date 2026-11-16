/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef AVID_LINKED_AUDIO_H
#define AVID_LINKED_AUDIO_H
#include "media.h"
/* Data is borrowed until callback returns; samples are interleaved floats.
 * Return nonzero to stop. start_seconds is the original container clock. */
typedef int (*AvidAudioCallback)(void *, const float *, size_t, int, double);
int avid_media_decode_audio(const char *, int, int, AvidAudioCallback, void *,
                            const AvidControl *, AvidError *);
#endif
