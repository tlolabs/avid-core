// Native fault injection against the actual pinned, patched Whisper SDK.
// SPDX-License-Identifier: GPL-3.0-or-later
#include "whisper_bridge.h"
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <new>
#include <fcntl.h>
#include <vector>

static std::atomic<long> live_allocations{0}, allocation_count{0}, fail_at{-1};
void *operator new(std::size_t size) {
    long index = allocation_count.fetch_add(1);
    if (index == fail_at.load()) throw std::bad_alloc();
    void *result = std::malloc(size ? size : 1);
    if (!result) throw std::bad_alloc();
    live_allocations.fetch_add(1);
    return result;
}
void *operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void *pointer) noexcept { if (pointer) { live_allocations.fetch_sub(1); std::free(pointer); } }
void operator delete[](void *pointer) noexcept { ::operator delete(pointer); }
void operator delete(void *pointer, std::size_t) noexcept { ::operator delete(pointer); }
void operator delete[](void *pointer, std::size_t) noexcept { ::operator delete(pointer); }
static int descriptors() { int count = 0; for (int i = 0; i < 2048; ++i) if (fcntl(i, F_GETFD) >= 0) ++count; return count; }
static int keep_going(void *) { return 0; }
static int segment(void *, const char *, double, double) { return 0; }
static void require(bool success, const char *what, long index, const AvidWhisperError &error) {
    if (!success) { std::fprintf(stderr, "FAIL %s at %ld: code=%d kind=%d %s live=%ld\n", what, index, error.code, error.kind, error.detail, live_allocations.load()); std::exit(2); }
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    AvidWhisperControl control{nullptr, keep_going}; AvidWhisperError error{};
    auto *samples = static_cast<float *>(std::calloc(16000, sizeof(float)));
    require(samples != nullptr, "sample fixture", -1, error);
    auto *warm = avid_whisper_open(argv[1], &control, &error);
    require(warm != nullptr, "warm real model", -1, error);
    // Settle CPU/FFT global caches before measuring per-operation ownership.
    require(avid_whisper_recognize(warm, samples, 16000, "en", 2, &control, segment, nullptr, nullptr, &error) == 0, "warm real inference", -1, error);
    avid_whisper_free(warm);
    allocation_count = 0;
    warm = avid_whisper_open(argv[1], &control, &error);
    long total = allocation_count.load();
    require(warm != nullptr && total > 2048, "measure real factory allocations", total, error);
    avid_whisper_free(warm);
    const long positions[] = {0, 1, 2, 8, 32, 128, 1024, total - 1, total - 8, total - 64, total - 512};
    int contained = 0, recovered = 0;
    for (long position : positions) {
        long baseline = live_allocations.load(); int files = descriptors();
        allocation_count = 0; fail_at = position;
        auto *model = avid_whisper_open(argv[1], &control, &error);
        fail_at = -1;
        if (model) {
            require(error.code == 0, "recovered factory has clean status", position, error);
            require(avid_whisper_recognize(model, samples, 16000, "en", 2, &control, segment, nullptr, nullptr, &error) == 0, "recovered factory performs real inference", position, error);
            ++recovered;
        }
        else { require(error.code != 0 && error.detail[0], "contain actual model allocation failure", position, error); ++contained; }
        avid_whisper_free(model);
        require(live_allocations.load() == baseline, "factory C++ allocations cleaned", position, error);
        require(descriptors() == files, "model FILE closed", position, error);
    }
    for (long position : {0L, 1L, 2L, 3L, 4L, 5L, 8L, 16L, 32L, 128L}) {
        long settled = live_allocations.load();
        auto *model = avid_whisper_open(argv[1], &control, &error);
        require(model != nullptr, "open inference model", position, error);
        long baseline = live_allocations.load();
        allocation_count = 0; fail_at = position;
        int result = avid_whisper_recognize(model, samples, 16000, "en", 2, &control, segment, nullptr, nullptr, &error);
        fail_at = -1;
        if (result != 0) {
            require((error.kind == 5 || error.kind == 3) && error.detail[0], "contain actual inference/worker allocation failure", position, error);
            result = avid_whisper_recognize(model, samples, 16000, "en", 2, &control, segment, nullptr, nullptr, &error);
            require(result == -10003 && error.kind == 8, "failed context cannot be reused", position, error);
        } else {
            require(error.code == 0, "recovered inference has clean status", position, error);
        }
        // Per-model allocations can legitimately grow before the failed stage;
        // destruction must return to the same settled process baseline.
        (void)baseline;
        avid_whisper_free(model);
        require(live_allocations.load() == settled, "inference C++ allocations cleaned after joining workers", position, error);
    }
    std::free(samples);
    require(contained > 2, "real allocation exceptions were exercised", contained, error);
    std::printf("PASS: %zu real factory faults (%d contained failures, %d recovered successes), 10 inference/worker faults; bounded diagnostics and FILE/context cleanup\n", sizeof(positions)/sizeof(*positions), contained, recovered);
}
