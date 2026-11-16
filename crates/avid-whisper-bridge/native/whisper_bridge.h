/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef AVID_WHISPER_BRIDGE_H
#define AVID_WHISPER_BRIDGE_H
#include <stddef.h>
#ifdef __cplusplus
#define AVID_NOEXCEPT noexcept
extern "C" {
#else
#define AVID_NOEXCEPT
#endif
typedef struct { void *opaque; int (*cancelled)(void *); } AvidWhisperControl;
typedef int (*AvidWhisperSegment)(void *, const char *, double, double);
typedef int (*AvidWhisperToken)(void *, const char *, double, double, float);
typedef struct { int code; int kind; char detail[512]; } AvidWhisperError;
typedef struct AvidWhisperModel AvidWhisperModel;
/* Error kinds: 1 validation, 2 I/O, 3 Whisper status, 4 cancellation,
 * 5 allocation exception, 6 standard C++ exception, 7 unknown exception,
 * 8 poisoned context. Callbacks must not unwind and remain valid until return. */
AvidWhisperModel *avid_whisper_open(const char *, const AvidWhisperControl *, AvidWhisperError *) AVID_NOEXCEPT;
void avid_whisper_free(AvidWhisperModel *) AVID_NOEXCEPT;
int avid_whisper_recognize(AvidWhisperModel *, const float *, int, const char *, int,
                          const AvidWhisperControl *, AvidWhisperSegment, AvidWhisperToken,
                          void *, AvidWhisperError *) AVID_NOEXCEPT;
#ifdef __cplusplus
}
#endif
#endif
