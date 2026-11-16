/* AVID Core ABI 1 / JSON schema 1. SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef AVID_CORE_H
#define AVID_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Integer opaque handles: 0 is invalid. Each successful create/result must be
 * released exactly once. Handles are type checked; never expose Rust layouts.
 * Jobs retain their own references, so release does not detach or kill a job. */
typedef uint64_t avid_handle;
typedef void (*avid_event_callback)(void *context, const uint8_t *json, size_t length);
uint32_t avid_core_abi_version(void);
avid_handle avid_core_create(void);
avid_handle avid_core_cancel_create(void);
int32_t avid_core_cancel(avid_handle cancellation);
int32_t avid_core_release(avid_handle handle);
size_t avid_core_buffer_len(avid_handle result);
/* out must designate writable capacity bytes. Returns 0 if too short/invalid.
 * JSON is UTF-8 with an explicit length, without a trailing NUL. */
size_t avid_core_buffer_copy(avid_handle result, uint8_t *out, size_t capacity);
/* Blocking call on the host worker thread; no detached operation survives return.
 * The request span must remain readable until return (maximum 4 MiB).
 * Events borrow UTF-8 bytes only for the callback. Copy to dispatch elsewhere.
 * Callback runs on the calling thread; it must return promptly and never unwind.
 * It may cancel, inspect buffers, release handles or launch an independent job;
 * it must not wait for this same operation or use an expired context.
 * Cancellation may be requested from any thread. Use one token per operation.
 * Native codec calls can delay cooperative cancellation; no hard deadline claim.
 * Result JSON: {schema_version:1,ok:true,result:...} or
 * {schema_version:1,ok:false,error:{code,detail}}. 0 means ABI allocation/panic error.
 * All foreign memory must be valid for the lengths supplied. An invalid nonnull
 * pointer is a caller error; Core cannot validate the caller's address space. */
avid_handle avid_core_execute_json(avid_handle core, avid_handle cancellation,
    const uint8_t *request, size_t length, avid_event_callback callback, void *context);
#ifdef __cplusplus
}
#endif
#endif
