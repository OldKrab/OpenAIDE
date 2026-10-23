package io.openaide.android;

import android.content.Context;
import android.util.Log;
import java.io.File;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Records metadata-only diagnostic events. Every part of the app writes to the
 * same process-wide log, so the report a user shares holds the connection
 * attempts and their timings, not only the events of the screen that shares it.
 */
final class AndroidDiagnostics {
    private static final DiagnosticsLog LOG = new DiagnosticsLog();

    /** Makes the log survive the process. Called once, when the app process starts. */
    static void install(Context context) {
        LOG.open(new File(new File(context.getFilesDir(), "diagnostics"), "phone.jsonl"));
    }

    void record(String event, String outcome, int count, long durationMs) {
        record(event, outcome, count, durationMs, java.util.Collections.emptyMap());
    }

    /** Adds metadata-only fields: numbers and fixed words, never user text, addresses or keys. */
    void record(String event, String outcome, int count, long durationMs, java.util.Map<String, ?> fields) {
        try {
            JSONObject json = new JSONObject()
                .put("timestamp", System.currentTimeMillis())
                .put("event", event)
                .put("outcome", outcome)
                .put("count", count)
                .put("duration_ms", durationMs);
            for (java.util.Map.Entry<String, ?> field : fields.entrySet()) json.put(field.getKey(), field.getValue());
            String entry = json.toString();
            LOG.append(entry);
            Log.i("OpenAIDE", entry);
        } catch (JSONException error) {
            Log.w("OpenAIDE", "diagnostics_serialization_failed");
        }
    }

    String snapshot() {
        return LOG.snapshot();
    }
}
