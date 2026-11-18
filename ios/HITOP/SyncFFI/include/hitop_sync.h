// hitop sync for the HITOP iOS app (ios/HITOP/SyncFFI, Rust): hitop-core's sync plan and folder
// format and hitop-sync-crypto behind one call. `op` is the operation, `args` its arguments as a
// JSON object; the result is {"ok": value} or {"error": message}, freed with hitop_sync_free.
#ifndef HITOP_SYNC_H
#define HITOP_SYNC_H

char *hitop_sync_call(const char *op, const char *args);
void hitop_sync_free(char *s);

#endif
