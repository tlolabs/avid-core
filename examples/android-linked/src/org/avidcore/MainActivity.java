package org.avidcore;

import android.app.Activity;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Color;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.widget.TextView;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Development-only on-device linked export/cancel/independent decode host. */
public final class MainActivity extends Activity {
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private volatile long activeToken;
    private volatile boolean interrupted;
    private TextView report;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        // Do not let an earlier successful run masquerade as this invocation.
        new File(getFilesDir(), "result.json").delete();
        report = new TextView(this); report.setText("Running linked media tests…"); setContentView(report);
        worker.execute(() -> {
            JSONObject result = new JSONObject();
            try {
                result.put("schema", 1).put("kind", "avid-linked-android-host-test").put("device", Build.MODEL)
                    .put("api", Build.VERSION.SDK_INT).put("supported_abis", new JSONArray(Build.SUPPORTED_ABIS));
                runTests(result);
                result.put("status", "passed");
            } catch (Throwable error) {
                try { result.put("status", "failed").put("error", error.toString()); } catch (Exception ignored) { }
            }
            try {
                // adb run-as can read a final report without screen interaction.
                File temporary = new File(getFilesDir(), "result.json.tmp"), output = new File(getFilesDir(), "result.json");
                try (FileOutputStream file = new FileOutputStream(temporary)) {
                    file.write(result.toString(2).getBytes(StandardCharsets.UTF_8)); file.getFD().sync();
                }
                Files.move(temporary.toPath(), output.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (Exception error) { Log.e("AVIDLinkedHost", "Could not write result.json", error); }
            String text = result.toString(); Log.i("AVIDLinkedHost", text);
            runOnUiThread(() -> { if (!isDestroyed()) report.setText(text); });
        });
    }

    private JSONObject call(LinkedCore core, JSONObject request) throws Exception {
        activeToken = LinkedCore.createCancellation();
        if (activeToken == 0) throw new IllegalStateException("Cancellation allocation failed");
        if (interrupted) LinkedCore.cancel(activeToken);
        try {
            byte[] bytes = core.execute(activeToken, request.toString().getBytes(StandardCharsets.UTF_8));
            if (bytes == null) throw new IllegalStateException("JNI execution failed");
            return new JSONObject(new String(bytes, StandardCharsets.UTF_8));
        } finally { LinkedCore.release(activeToken); activeToken = 0; }
    }

    private JSONObject checked(LinkedCore core, JSONObject request) throws Exception {
        JSONObject response = call(core, request);
        if (!response.getBoolean("ok")) throw new IllegalStateException(response.toString());
        return response;
    }

    private void runTests(JSONObject report) throws Exception {
        File image = new File(getCacheDir(), "cover ü.png"), audio = new File(getCacheDir(), "audio.wav");
        Bitmap bitmap = Bitmap.createBitmap(96, 64, Bitmap.Config.ARGB_8888); bitmap.eraseColor(Color.RED);
        try (FileOutputStream file = new FileOutputStream(image)) {
            if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, file)) throw new IllegalStateException("PNG fixture failed");
        } finally { bitmap.recycle(); }
        int rate = 44100, samples = rate * 2;
        ByteBuffer wav = ByteBuffer.allocate(44 + samples * 2).order(ByteOrder.LITTLE_ENDIAN);
        wav.put("RIFF".getBytes(StandardCharsets.US_ASCII)).putInt(36 + samples * 2).put("WAVEfmt ".getBytes(StandardCharsets.US_ASCII))
            .putInt(16).putShort((short)1).putShort((short)1).putInt(rate).putInt(rate * 2).putShort((short)2).putShort((short)16)
            .put("data".getBytes(StandardCharsets.US_ASCII)).putInt(samples * 2);
        for (int i = 0; i < samples; i++) wav.putShort((short)(12000 * Math.sin(i * 2 * Math.PI * 440 / rate)));
        try (FileOutputStream file = new FileOutputStream(audio)) { file.write(wav.array()); }
        final int[] progress = {0}; final boolean[] cancelNext = {false};
        try (LinkedCore core = new LinkedCore(event -> {
            try {
                if (new JSONObject(new String(event, StandardCharsets.UTF_8)).optString("event").equals("progress")) {
                    progress[0]++; if (cancelNext[0]) LinkedCore.cancel(activeToken);
                }
            } catch (Exception error) { throw new IllegalStateException(error); }
        })) {
            report.put("capabilities", checked(core, new JSONObject().put("schema_version", 1).put("operation", "capabilities")));
            report.put("probe", checked(core, new JSONObject().put("schema_version", 1).put("operation", "probe_audio").put("audio", audio.getPath())));
            JSONArray exports = new JSONArray();
            for (String composition : new String[]{"fitted", "square_padded"}) {
                JSONObject settings = new JSONObject().put("width", 160).put("height", 90).put("fps", 24).put("encoding", "automatic").put("composition", composition);
                File preview = new File(getCacheDir(), composition + ".png");
                checked(core, new JSONObject().put("schema_version", 1).put("operation", "preview").put("image", image.getPath()).put("output", preview.getPath()).put("settings", settings));
                Bitmap decoded = BitmapFactory.decodeFile(preview.getPath());
                if (decoded == null) throw new IllegalStateException("PNG preview did not decode");
                try { if (decoded.getWidth() != 160 || decoded.getHeight() != 90) throw new IllegalStateException("Preview geometry mismatch"); } finally { decoded.recycle(); }
                for (String mode : new String[]{"simple", "per_frame"}) {
                    File output = new File(getCacheDir(), composition + "-" + mode + ".mp4");
                    JSONObject request = new JSONObject().put("schema_version", 1).put("operation", "render").put("image", image.getPath())
                        .put("audio", audio.getPath()).put("output", output.getPath()).put("mode", mode).put("settings", settings);
                    if (mode.equals("per_frame")) {
                        JSONArray clips = new JSONArray();
                        for (int i = 0; i < 2; i++) clips.put(new JSONObject().put("id", "clip-" + i).put("image", image.getPath()).put("audio", audio.getPath()).put("duration_seconds", 1));
                        request.remove("image"); request.remove("audio"); request.put("clips", clips);
                    }
                    JSONObject response = checked(core, request);
                    exports.put(new JSONObject().put("composition", composition).put("mode", mode).put("core", response).put("decode", DecodedMedia.inspect(output, mode.equals("per_frame"))));
                }
            }
            report.put("exports", exports).put("progress_events", progress[0]);
            if (progress[0] == 0) throw new IllegalStateException("No progress callback");
            File output = new File(getCacheDir(), "cancel.mp4"); byte[] sentinel = "existing destination".getBytes(StandardCharsets.UTF_8);
            Files.write(output.toPath(), sentinel); cancelNext[0] = true;
            JSONObject result = call(core, new JSONObject().put("schema_version", 1).put("operation", "render").put("image", image.getPath())
                .put("audio", audio.getPath()).put("output", output.getPath()).put("settings", new JSONObject().put("width", 160).put("height", 90).put("encoding", "automatic")));
            if (result.getBoolean("ok") || !result.getJSONObject("error").getString("code").equals("cancelled")) throw new IllegalStateException("Cancellation failed: " + result);
            if (!java.util.Arrays.equals(sentinel, Files.readAllBytes(output.toPath()))) throw new IllegalStateException("Destination changed after cancellation");
            report.put("cancellation", result).put("destination_preserved", true);
        }
    }

    @Override public void onDestroy() {
        interrupted = true;
        long token = activeToken; if (token != 0) LinkedCore.cancel(token);
        worker.shutdown(); // Do not detach/kill an executing native operation.
        super.onDestroy();
    }
}
