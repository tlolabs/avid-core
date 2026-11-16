/* SPDX-FileCopyrightText: Thomas Lothian
 * SPDX-License-Identifier: GPL-3.0-or-later */
#include "whisper_bridge.h"
#include <whisper.h>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <memory>
#include <new>
#ifdef _WIN32
#include <windows.h>
#endif

struct AvidWhisperModel { whisper_context *context; bool poisoned; };
namespace {
constexpr int allocation_error = -10000;
constexpr int standard_exception = -10001;
constexpr int unknown_exception = -10002;
constexpr int poisoned_error = -10003;
static int fail(AvidWhisperError *error, int code, int kind, const char *detail) noexcept {
    if (error) { error->code = code; error->kind = kind; std::snprintf(error->detail, sizeof(error->detail), "%s", detail ? detail : "native error"); }
    return code;
}
static void clear(AvidWhisperError *error) noexcept { if (error) std::memset(error, 0, sizeof(*error)); }
static int cancelled(const AvidWhisperControl *control) noexcept {
    // Rust callbacks contain their own panics. A foreign nonconforming callback
    // is treated as cancellation, including on SDK abort-callback threads.
    try { return control && control->cancelled && control->cancelled(control->opaque); }
    catch (...) { return 1; }
}
static bool abort_inference(void *opaque) noexcept { return cancelled(static_cast<const AvidWhisperControl *>(opaque)) != 0; }
struct FileCloser { void operator()(FILE *file) const noexcept { if (file) std::fclose(file); } };
using File = std::unique_ptr<FILE, FileCloser>;
struct ModelReader { File file; const AvidWhisperControl *control; };
static size_t model_read(void *opaque, void *out, size_t size) noexcept {
    auto *reader = static_cast<ModelReader *>(opaque);
    return cancelled(reader->control) ? 0 : std::fread(out, 1, size, reader->file.get());
}
static bool model_eof(void *opaque) noexcept {
    auto *reader = static_cast<ModelReader *>(opaque);
    return cancelled(reader->control) || std::feof(reader->file.get()) || std::ferror(reader->file.get());
}
static void model_close(void *opaque) noexcept { static_cast<ModelReader *>(opaque)->file.reset(); }
static FILE *open_model(const char *path) noexcept {
#ifdef _WIN32
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, nullptr, 0);
    if (size <= 0) return nullptr;
    auto *wide = static_cast<wchar_t *>(std::calloc(static_cast<size_t>(size), sizeof(wchar_t)));
    if (!wide) return nullptr;
    if (!MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, size)) { std::free(wide); return nullptr; }
    FILE *file = _wfopen(wide, L"rb"); std::free(wide); return file;
#else
    return std::fopen(path, "rb");
#endif
}
struct ContextCloser { void operator()(whisper_context *context) const noexcept { if (context) whisper_free(context); } };
using Context = std::unique_ptr<whisper_context, ContextCloser>;
}

extern "C" AvidWhisperModel *avid_whisper_open(const char *path, const AvidWhisperControl *control, AvidWhisperError *error) noexcept {
    clear(error);
    try {
        if (!path || std::strcmp(whisper_version(), "1.8.2")) { fail(error, -1, 1, "invalid model path or unsupported Whisper version"); return nullptr; }
        if (cancelled(control)) { fail(error, -3, 4, "operation cancelled"); return nullptr; }
        ModelReader reader{File(open_model(path)), control};
        if (!reader.file) { fail(error, -2, 2, "could not open local model file"); return nullptr; }
        whisper_model_loader loader{};
        loader.context = &reader; loader.read = model_read; loader.eof = model_eof; loader.close = model_close;
        whisper_context_params params = whisper_context_default_params();
        params.use_gpu = false;
        params.dtw_token_timestamps = false; // Experimental DTW is not this CPU recognition contract.
        Context context(whisper_init_with_params(&loader, params));
        if (cancelled(control)) { fail(error, -3, 4, "operation cancelled"); return nullptr; }
        if (!context) { fail(error, -4, 3, "Whisper could not initialize the verified local model"); return nullptr; }
        std::unique_ptr<AvidWhisperModel> model(new AvidWhisperModel{context.get(), false});
        context.release();
        return model.release();
    } catch (const std::bad_alloc &) { fail(error, allocation_error, 5, "C++ allocation failed while loading Whisper model"); }
      catch (const std::exception &exception) { fail(error, standard_exception, 6, exception.what()); }
      catch (...) { fail(error, unknown_exception, 7, "unknown C++ exception while loading Whisper model"); }
    return nullptr;
}
extern "C" void avid_whisper_free(AvidWhisperModel *model) noexcept {
    if (!model) return;
    // The pinned CPU cleanup path contains only nonthrowing destructors/frees;
    // factory RAII patch ensures every partially initialized member is valid.
    Context context(model->context);
    delete model;
}
extern "C" int avid_whisper_recognize(AvidWhisperModel *model, const float *samples, int count,
    const char *language, int threads, const AvidWhisperControl *control,
    AvidWhisperSegment segment, AvidWhisperToken token, void *opaque, AvidWhisperError *error) noexcept {
    clear(error);
    try {
        if (!model || !samples || count <= 0 || count > 16000 * 30 || threads < 1 || threads > 64 || !segment)
            return fail(error, -1, 1, "expected a model, <=30 seconds of PCM, 1..64 threads and a segment callback");
        if (model->poisoned) return fail(error, poisoned_error, 8, "Whisper context must be unloaded after an inference exception");
        auto *context = model->context;
        if (language && (whisper_lang_id(language) < 0 || (std::strcmp(language, "en") && !whisper_is_multilingual(context))))
            return fail(error, -2, 1, "language is not supported by this model");
        if (cancelled(control)) return fail(error, -3, 4, "operation cancelled");
        whisper_full_params params = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
        params.n_threads = threads; params.language = language; params.detect_language = language == nullptr;
        params.translate = false; params.no_context = true; params.print_progress = false;
        params.print_realtime = false; params.print_timestamps = false; params.print_special = false;
        params.token_timestamps = true; params.abort_callback = abort_inference; params.abort_callback_user_data = const_cast<AvidWhisperControl *>(control);
        // Upstream reflected FFT padding reads 200 samples from the input.
        // Very short valid clips get bounded zero padding rather than an OOB read.
        float short_input[201]{};
        if (count < 201) { std::memcpy(short_input, samples, static_cast<size_t>(count) * sizeof(float)); samples = short_input; count = 201; }
        int status = whisper_full(context, params, samples, count);
        if (cancelled(control)) return fail(error, -3, 4, "operation cancelled");
        if (status) { model->poisoned = true; return fail(error, status, 3, "Whisper recognition returned a native failure status"); }
        for (int i = 0; i < whisper_full_n_segments(context); ++i) {
            const char *text = whisper_full_get_segment_text(context, i);
            double start = static_cast<double>(whisper_full_get_segment_t0(context, i)) / 100;
            double end = static_cast<double>(whisper_full_get_segment_t1(context, i)) / 100;
            if (!text || !std::isfinite(start) || !std::isfinite(end) || start < 0 || end < start)
                return fail(error, -4, 3, "Whisper returned invalid segment timing");
            if (segment(opaque, text, start, end)) return fail(error, -3, 4, "host stopped recognition delivery");
            for (int j = 0; token && j < whisper_full_n_tokens(context, i); ++j) {
                whisper_token_data data = whisper_full_get_token_data(context, i, j);
                if (data.id >= whisper_token_eot(context)) continue;
                const char *piece = whisper_full_get_token_text(context, i, j);
                if (!piece || token(opaque, piece, static_cast<double>(data.t0) / 100, static_cast<double>(data.t1) / 100, data.p))
                    return fail(error, -3, 4, "host stopped token delivery");
            }
        }
        return 0;
    } catch (const std::bad_alloc &) { if (model) model->poisoned = true; return fail(error, allocation_error, 5, "C++ allocation failed during Whisper inference"); }
      catch (const std::exception &exception) { if (model) model->poisoned = true; return fail(error, standard_exception, 6, exception.what()); }
      catch (...) { if (model) model->poisoned = true; return fail(error, unknown_exception, 7, "unknown C++ exception during Whisper inference"); }
}
