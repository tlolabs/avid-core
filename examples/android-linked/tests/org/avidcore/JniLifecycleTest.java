package org.avidcore;

import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import javax.imageio.ImageIO;

/** Desktop JVM transport regression, outside the Android source set.
 * This exercises real JNI/Core export and foreign exception teardown; it is
 * not Android compilation or device qualification. */
public final class JniLifecycleTest {
    private static final class CallbackFailure extends RuntimeException {
        private static final long serialVersionUID = 1L;
    }
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
    private static String quote(Path path) {
        return "\"" + path.toString().replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }
    private static byte[] audio() {
        int rate = 44100, samples = rate * 2;
        ByteBuffer bytes = ByteBuffer.allocate(44 + samples * 2).order(ByteOrder.LITTLE_ENDIAN);
        bytes.put("RIFF".getBytes(StandardCharsets.US_ASCII)).putInt(36 + samples * 2)
            .put("WAVEfmt ".getBytes(StandardCharsets.US_ASCII)).putInt(16).putShort((short)1)
            .putShort((short)1).putInt(rate).putInt(rate * 2).putShort((short)2).putShort((short)16)
            .put("data".getBytes(StandardCharsets.US_ASCII)).putInt(samples * 2);
        for (int i = 0; i < samples; i++) bytes.putShort((short)(12000 * Math.sin(i * 2 * Math.PI * 440 / rate)));
        return bytes.array();
    }
    @SuppressWarnings("try") // Deliberately exercise idempotent close inside try-with-resources.
    public static void main(String[] args) throws Exception {
        require(args.length == 1, "Pass an existing, empty work directory");
        Path directory = Path.of(args[0]);
        Path image = directory.resolve("artwork ü.png"), audio = directory.resolve("tone.wav"), output = directory.resolve("jni ü.mp4");
        BufferedImage art = new BufferedImage(32, 16, BufferedImage.TYPE_INT_RGB);
        for (int y = 0; y < 16; y++) for (int x = 0; x < 32; x++) art.setRGB(x, y, 0xff0000);
        try (ByteArrayOutputStream png = new ByteArrayOutputStream()) {
            require(ImageIO.write(art, "png", png), "PNG fixture encoder missing");
            Files.write(image, png.toByteArray());
        }
        Files.write(audio, audio());
        byte[] request = ("{\"schema_version\":1,\"operation\":\"render\",\"image\":" + quote(image)
            + ",\"audio\":" + quote(audio) + ",\"output\":" + quote(output)
            + ",\"mode\":\"simple\",\"settings\":{\"width\":160,\"height\":90,\"fps\":24,\"encoding\":\"software\"}}")
            .getBytes(StandardCharsets.UTF_8);
        AtomicBoolean fail = new AtomicBoolean(true);
        AtomicInteger progress = new AtomicInteger();
        Thread caller = Thread.currentThread();
        try (LinkedCore core = new LinkedCore(event -> {
            require(Thread.currentThread() == caller, "JNI callback changed thread");
            if (new String(event, StandardCharsets.UTF_8).contains("\"event\":\"progress\"")) {
                progress.incrementAndGet();
                if (fail.get()) throw new CallbackFailure();
            }
        })) {
            byte[] sentinel = "existing destination".getBytes(StandardCharsets.UTF_8);
            Files.write(output, sentinel);
            long token = LinkedCore.createCancellation();
            require(token != 0, "Cancellation creation failed");
            try {
                core.execute(token, request);
                throw new AssertionError("Callback exception was swallowed");
            } catch (CallbackFailure expected) {
                require(progress.get() == 1, "Callbacks continued after Java exception");
                require(Arrays.equals(Files.readAllBytes(output), sentinel), "Exception replaced destination");
            } finally { LinkedCore.release(token); }
            try (java.util.stream.Stream<Path> files = Files.list(directory)) {
                require(files.noneMatch(p -> p.getFileName().toString().startsWith(".avid-")), "Native staging file survived return");
            }
            fail.set(false);
            token = LinkedCore.createCancellation();
            try {
                String result = new String(core.execute(token, request), StandardCharsets.UTF_8);
                require(result.contains("\"ok\":true"), "Subsequent export failed: " + result);
                require(Files.size(output) > sentinel.length, "No MP4 output");
                require(progress.get() > 1, "Subsequent export had no progress");
            } finally { LinkedCore.release(token); }
            core.close(); core.close();
            try {
                core.execute(0, request);
                throw new AssertionError("Closed Core accepted an operation");
            } catch (IllegalStateException expected) { }
        }
        System.out.println("PASS: real JNI export, synchronous progress, Java exception cancellation, cleanup, destination preservation and repeated export");
    }
}
