package org.avidcore;

/** Blocking JNI transport. Run execute on a host worker, keep this instance alive,
 * copy imported content:// data into app-private storage before calling Core. */
public final class LinkedCore implements AutoCloseable {
    static { System.loadLibrary("avid_core_jni"); }
    private final java.util.concurrent.atomic.AtomicLong handle;
    private final java.util.function.Consumer<byte[]> events;
    public LinkedCore(java.util.function.Consumer<byte[]> events) {
        this.events = java.util.Objects.requireNonNull(events);
        if (abiVersion() != 1) throw new IllegalStateException("Unsupported AVID Core ABI");
        long value = create();
        if (value == 0) throw new IllegalStateException("Core allocation failed");
        handle = new java.util.concurrent.atomic.AtomicLong(value);
    }
    private static native int abiVersion();
    private static native long create();
    public static native long createCancellation();
    public static native void cancel(long token);
    public static native void release(long handle);
    private native byte[] execute(long core, long token, byte[] request);
    public byte[] execute(long token, byte[] request) {
        long value = handle.get();
        if (value == 0) throw new IllegalStateException("Core is closed");
        byte[] response = execute(value, token, java.util.Objects.requireNonNull(request));
        if (response == null) throw new IllegalStateException("Core JNI execution failed");
        return response;
    }
    // Called synchronously on the execute caller; no Java exception should escape.
    private void onEvent(byte[] json) { events.accept(json); }
    // The host must wait for all execute calls before closing or unloading libraries.
    @Override public void close() { long value = handle.getAndSet(0); if (value != 0) release(value); }
}
