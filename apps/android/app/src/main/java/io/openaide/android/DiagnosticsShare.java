package io.openaide.android;

import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import androidx.core.content.FileProvider;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;

/**
 * Shares the diagnostics snapshot as a named file, so a chat or mail app attaches it instead of
 * pasting hundreds of lines as a message. The file is rewritten on every share and holds only the
 * metadata-only entries {@link DiagnosticsLog} keeps.
 */
final class DiagnosticsShare {
    static final String DIRECTORY = "diagnostics";
    static final String FILE_NAME = "openaide-phone-diagnostics.jsonl";

    private DiagnosticsShare() {}

    /** The chooser that shares the snapshot; throws when the cache file cannot be written. */
    static Intent chooser(Context context, String snapshot) throws IOException {
        File file = write(new File(context.getCacheDir(), DIRECTORY), snapshot);
        Uri uri = FileProvider.getUriForFile(context, context.getPackageName() + ".captures", file);
        Intent send = new Intent(Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .putExtra(Intent.EXTRA_SUBJECT, "OpenAIDE phone diagnostics")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        // The chooser and the receiving app both read the grant from the clip data.
        send.setClipData(ClipData.newRawUri(FILE_NAME, uri));
        return Intent.createChooser(send, "Share diagnostics").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
    }

    /** Replaces the shared file with the snapshot, one entry per line. */
    static File write(File directory, String snapshot) throws IOException {
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Diagnostics directory unavailable");
        File file = new File(directory, FILE_NAME);
        try (OutputStream out = new FileOutputStream(file, false)) {
            out.write(snapshot.getBytes(StandardCharsets.UTF_8));
            if (!snapshot.isEmpty()) out.write('\n');
        }
        return file;
    }
}
