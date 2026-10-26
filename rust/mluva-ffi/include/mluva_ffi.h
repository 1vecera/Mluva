// C ABI of rust/mluva-ffi. Requests and responses are UTF-8 JSON; see src/lib.rs.
#ifndef MLUVA_FFI_H
#define MLUVA_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*mluva_event_handler)(void *context, const char *event);

// Run {"method": ..., "params": {...}} and return {"ok": ...} or {"error": "..."}.
char *mluva_call(const char *request);
void mluva_string_free(char *value);
// Events arrive on runtime threads; the handler must dispatch to its own queue.
void mluva_set_event_handler(mluva_event_handler handler, void *context);
// Queue 16 kHz, 16-bit little-endian mono PCM for a realtime session.
bool mluva_realtime_audio(const char *session, const uint8_t *bytes, size_t length);

#ifdef __cplusplus
}
#endif

#endif
