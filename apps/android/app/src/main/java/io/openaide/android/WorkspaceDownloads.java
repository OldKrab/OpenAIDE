package io.openaide.android;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.DocumentsContract;
import android.provider.MediaStore;
import android.util.Log;
import android.webkit.DownloadListener;
import android.widget.Toast;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Saves the files the workspace page downloads. The Web Shell starts every download
 * (File Viewer, sent files, support exports) with an `<a download>` click, and a
 * WebView saves nothing itself: it hands the request to this listener.
 *
 * The app fetches the file again through its own gateway, with the cookie only its
 * WebView holds, and writes it to Downloads (Android 10+) or to the place the user
 * picks (Android 8–9, which would otherwise need a storage permission). The gateway
 * token never leaves the process, so the system DownloadManager is not used.
 */
final class WorkspaceDownloads implements DownloadListener {
    static final int SAVE_REQUEST = 6;
    private static final int CHUNK_BYTES = 64 * 1024;
    private static final AtomicInteger operations = new AtomicInteger();
    /** Process-wide, so a transfer outlives the Activity that started it. */
    private static final ExecutorService transfers = Executors.newSingleThreadExecutor(task -> {
        Thread thread = new Thread(task, "openaide-download");
        thread.setDaemon(true);
        return thread;
    });
    private static final Handler main = new Handler(Looper.getMainLooper());

    private final Activity activity;
    private final ConnectionProfile profile;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    /** A download waiting for the user to pick its destination; Android 8–9 only. */
    private Request awaitingDestination;

    WorkspaceDownloads(Activity activity, ConnectionProfile profile) {
        this.activity = activity;
        this.profile = profile;
    }

    private static final class Request {
        final int operation;
        final long started;
        final String url;
        final String name;
        final String type;
        Request(int operation, long started, String url, String name, String type) {
            this.operation = operation;
            this.started = started;
            this.url = url;
            this.name = name;
            this.type = type;
        }
    }

    @Override public void onDownloadStart(String url, String userAgent, String disposition, String mimeType, long length) {
        int operation = operations.incrementAndGet();
        long started = SystemClock.elapsedRealtime();
        diagnostics.record("workspace_download", "started", operation, 0);
        // The Frontend downloads only from its own origin; anything else is not a workspace file.
        if (!profile.owns(url)) {
            finish(activity.getApplicationContext(), operation, started, "rejected", 0, 0, "This download is not supported.");
            return;
        }
        String type = mimeType == null || mimeType.isEmpty() ? "application/octet-stream" : mimeType.split(";")[0].trim();
        Request request = new Request(operation, started, url, DownloadName.from(disposition, url), type);
        if (Build.VERSION.SDK_INT >= 29) {
            transfers.execute(() -> transfer(activity.getApplicationContext(), request, null));
            return;
        }
        if (awaitingDestination != null) {
            finish(activity.getApplicationContext(), awaitingDestination.operation, awaitingDestination.started, "replaced", 0, 0, null);
        }
        awaitingDestination = request;
        try {
            activity.startActivityForResult(new Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE)
                .setType(type).putExtra(Intent.EXTRA_TITLE, request.name), SAVE_REQUEST);
        } catch (android.content.ActivityNotFoundException error) {
            awaitingDestination = null;
            finish(activity.getApplicationContext(), operation, started, "picker_unavailable", 0, 0, "This phone cannot save files here.");
        }
    }

    /** The destination the user picked on Android 8–9, or none when they cancelled. */
    void onActivityResult(int result, Intent data) {
        Request request = awaitingDestination;
        awaitingDestination = null;
        if (request == null) return;
        Uri destination = result == Activity.RESULT_OK && data != null ? data.getData() : null;
        if (destination == null) {
            finish(activity.getApplicationContext(), request.operation, request.started, "cancelled", 0, 0, null);
            return;
        }
        transfers.execute(() -> transfer(activity.getApplicationContext(), request, destination));
    }

    /** Drops a pending pick whose result can no longer arrive. */
    void dispose() {
        if (awaitingDestination != null) {
            finish(activity.getApplicationContext(), awaitingDestination.operation, awaitingDestination.started, "activity_destroyed", 0, 0, null);
            awaitingDestination = null;
        }
    }

    /** Streams the file from the gateway into Downloads, or into the picked document when one is given. */
    private void transfer(Context context, Request request, Uri picked) {
        ContentResolver resolver = context.getContentResolver();
        HttpURLConnection connection = null;
        Uri destination = picked;
        long copied = 0;
        int status = 0;
        String outcome = "saved";
        boolean writing = false;
        try {
            connection = (HttpURLConnection) new URL(request.url).openConnection();
            connection.setConnectTimeout(5_000);
            connection.setReadTimeout(30_000);
            connection.setInstanceFollowRedirects(false);
            connection.setRequestProperty("Cookie", GatewayHttp.COOKIE + "=" + WorkspaceGateway.INSTANCE.getToken());
            status = connection.getResponseCode();
            if (status != 200) {
                outcome = status == 404 ? "not_found" : status == 401 || status == 403 ? "permission_denied"
                    : status == 400 ? "not_a_file" : "http_error";
                return;
            }
            writing = true;
            if (destination == null) destination = createDownload(resolver, request);
            if (destination == null) throw new IOException("Downloads unavailable");
            try (InputStream input = connection.getInputStream(); OutputStream output = resolver.openOutputStream(destination, "w")) {
                if (output == null) throw new IOException("Destination unavailable");
                byte[] buffer = new byte[CHUNK_BYTES];
                while (true) {
                    writing = false;
                    int count = input.read(buffer);
                    if (count == -1) break;
                    writing = true;
                    output.write(buffer, 0, count);
                    copied += count;
                }
            }
            writing = false;
            long expected = connection.getContentLengthLong();
            if (expected >= 0 && copied != expected) throw new IOException("Truncated transfer");
            writing = true;
            if (picked == null) {
                ContentValues done = new ContentValues();
                done.put(MediaStore.MediaColumns.IS_PENDING, 0);
                resolver.update(destination, done, null, null);
            }
        } catch (IOException | RuntimeException error) {
            outcome = writing ? "storage_failed" : "transfer_failed";
        } finally {
            if (connection != null) connection.disconnect();
            if (!"saved".equals(outcome) && destination != null) discard(resolver, destination, picked != null);
            String message = switch (outcome) {
                case "saved" -> picked == null ? "Saved " + request.name + " to Downloads." : "Saved " + request.name + ".";
                case "not_found" -> "File not found. It may have moved or been deleted.";
                case "permission_denied" -> "Access denied. Check file permissions and your connection.";
                case "not_a_file" -> "Only regular files can be downloaded.";
                case "storage_failed" -> "Could not save " + request.name + " on this phone. Check free storage and try again.";
                default -> "Download failed. Check your connection and try again.";
            };
            finish(context, request.operation, request.started, outcome, status, copied, message);
        }
    }

    /** A pending Downloads entry, hidden from other apps until its bytes are complete. */
    private static Uri createDownload(ContentResolver resolver, Request request) {
        if (Build.VERSION.SDK_INT < 29) return null;
        ContentValues values = new ContentValues();
        values.put(MediaStore.MediaColumns.DISPLAY_NAME, request.name);
        values.put(MediaStore.MediaColumns.MIME_TYPE, request.type);
        values.put(MediaStore.MediaColumns.IS_PENDING, 1);
        return resolver.insert(MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), values);
    }

    private static void discard(ContentResolver resolver, Uri destination, boolean picked) {
        try {
            if (picked) DocumentsContract.deleteDocument(resolver, destination);
            else resolver.delete(destination, null, null);
        } catch (Exception error) {
            Log.w("OpenAIDE", "workspace_download_discard_failed");
        }
    }

    /** Terminal event: metadata only, never the file name or URL. */
    private void finish(Context context, int operation, long started, String outcome, int status, long bytes, String message) {
        long duration = SystemClock.elapsedRealtime() - started;
        diagnostics.record("workspace_download", outcome, operation, duration);
        Log.i("OpenAIDE", "workspace_download_end operation=" + operation + " outcome=" + outcome
            + " http_status=" + status + " bytes=" + bytes + " duration_ms=" + duration);
        if (message != null) main.post(() -> Toast.makeText(context, message, Toast.LENGTH_LONG).show());
    }
}
