package org.avidcore;

/** Blocking JNI transport. Run execute on a host worker, keep this instance alive,
 * copy imported content:// data into app-private storage before calling Core. */
public final class LinkedCore implements AutoCloseable {
    static { System.loadLibrary("avid_core_jni"); }
    private final long handle = create();
    private final java.util.function.Consumer<byte[]> events;
    public LinkedCore(java.util.function.Consumer<byte[]> events) { this.events = events; }
    private static native long create();
    public static native long createCancellation();
    public static native void cancel(long token);
    public static native void release(long handle);
    private native byte[] execute(long core, long token, byte[] request);
    public byte[] execute(long token, byte[] request) { return execute(handle, token, request); }
    // Called synchronously on the execute caller; no Java exception should escape.
    private void onEvent(byte[] json) { events.accept(json); }
    // The host must wait for all execute calls before closing or unloading libraries.
    @Override public void close() { release(handle); }
}
