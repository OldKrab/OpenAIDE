package io.openaide.android;

import android.util.Log;
import java.util.ArrayDeque;
import org.json.JSONException;
import org.json.JSONObject;

final class AndroidDiagnostics {
    private final ArrayDeque<String> entries = new ArrayDeque<>();

    synchronized void record(String event, String outcome, int count, long durationMs) {
        try {
            String entry = new JSONObject()
                .put("timestamp", System.currentTimeMillis())
                .put("event", event)
                .put("outcome", outcome)
                .put("count", count)
                .put("duration_ms", durationMs).toString();
            if (entries.size() == 100) entries.removeFirst();
            entries.addLast(entry);
            Log.i("OpenAIDE", entry);
        } catch (JSONException error) {
            Log.w("OpenAIDE", "diagnostics_serialization_failed");
        }
    }

    synchronized String snapshot() {
        return String.join("\n", entries);
    }
}
