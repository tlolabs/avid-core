package org.avidcore;

import android.app.Activity;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.media.MediaExtractor;
import android.os.Bundle;
import android.widget.TextView;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Development-only on-device linked export/cancel/MP4 inspection host. */
public final class MainActivity extends Activity {
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private volatile long activeToken;
    private TextView report;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state); report=new TextView(this); report.setText("Running linked media tests…"); setContentView(report);
        worker.execute(()->{try {String result=runTests();runOnUiThread(()->report.setText(result));}catch(Exception e){runOnUiThread(()->report.setText("FAILED: "+e));}});
    }
    private String runTests() throws Exception {
        File image=new File(getCacheDir(),"cover ü.png"), audio=new File(getCacheDir(),"audio.wav"), output=new File(getCacheDir(),"linked.mp4");
        Bitmap bitmap=Bitmap.createBitmap(96,64,Bitmap.Config.ARGB_8888);bitmap.eraseColor(Color.RED);
        try(FileOutputStream file=new FileOutputStream(image)){bitmap.compress(Bitmap.CompressFormat.PNG,100,file);}bitmap.recycle();
        int rate=44100,samples=rate*2;ByteBuffer wav=ByteBuffer.allocate(44+samples*2).order(ByteOrder.LITTLE_ENDIAN);
        wav.put("RIFF".getBytes("US-ASCII")).putInt(36+samples*2).put("WAVEfmt ".getBytes("US-ASCII")).putInt(16).putShort((short)1).putShort((short)1).putInt(rate).putInt(rate*2).putShort((short)2).putShort((short)16).put("data".getBytes("US-ASCII")).putInt(samples*2);
        for(int i=0;i<samples;i++)wav.putShort((short)(12000*Math.sin(i*2*Math.PI*440/rate)));
        try(FileOutputStream file=new FileOutputStream(audio)){file.write(wav.array());}
        JSONObject settings=new JSONObject().put("width",160).put("height",90).put("fps",24).put("encoding","automatic");
        byte[] request=new JSONObject().put("schema_version",1).put("operation","render").put("image",image.getPath()).put("audio",audio.getPath()).put("output",output.getPath()).put("mode","simple").put("settings",settings).toString().getBytes("UTF-8");
        final int[] progress={0}; final boolean[] cancelNext={false};
        try(LinkedCore core=new LinkedCore(event->{try {if(new JSONObject(new String(event,"UTF-8")).optString("event").equals("progress")){progress[0]++;if(cancelNext[0])LinkedCore.cancel(activeToken);}}catch(Exception error){throw new IllegalStateException(error);}})) {
            activeToken=LinkedCore.createCancellation();JSONObject result;
            try {result=new JSONObject(new String(core.execute(activeToken,request),"UTF-8"));}finally{LinkedCore.release(activeToken);activeToken=0;}
            if(!result.getBoolean("ok")||progress[0]==0)throw new IllegalStateException(result.toString());
            byte[] before=java.nio.file.Files.readAllBytes(output.toPath());
            MediaExtractor extractor=new MediaExtractor();try{extractor.setDataSource(output.getPath());if(extractor.getTrackCount()<2)throw new IllegalStateException("missing AV tracks");}finally{extractor.release();}
            cancelNext[0]=true;activeToken=LinkedCore.createCancellation();
            try{result=new JSONObject(new String(core.execute(activeToken,request),"UTF-8"));}finally{LinkedCore.release(activeToken);activeToken=0;}
            if(result.getBoolean("ok")||!result.getJSONObject("error").getString("code").equals("cancelled"))throw new IllegalStateException("cancellation failed: "+result);
            if(!java.util.Arrays.equals(before,java.nio.file.Files.readAllBytes(output.toPath())))throw new IllegalStateException("destination changed after cancellation");
        }
        return "PASS: linked export, progress, cancellation, destination preservation and MP4 tracks. Playback/decoded-pixel validation still required.";
    }
    @Override public void onDestroy(){long token=activeToken;if(token!=0)LinkedCore.cancel(token);worker.shutdown();super.onDestroy();}
}
