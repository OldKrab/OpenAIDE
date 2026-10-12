package io.openaide.android;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;

/**
 * The one list of diagnostic entries for the whole app process, kept across
 * restarts. A slow or failed connection is usually reported after Android has
 * stopped and restarted the app, so entries held only in memory were gone by
 * the time the user shared them.
 *
 * Entries are short metadata lines. Each one is written through at once: the
 * process can be killed at any moment and the last lines are the useful ones.
 */
final class DiagnosticsLog {
    static final int CAPACITY = 400;

    private final ArrayDeque<String> entries = new ArrayDeque<>();
    private File file;
    private Writer writer;
    private int written;

    /** Loads what earlier runs left and starts appending to the same file. Later calls change nothing. */
    synchronized void open(File target) {
        if (file != null) return;
        file = target;
        ArrayDeque<String> earlier = new ArrayDeque<>();
        if (target.isFile()) {
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(new java.io.FileInputStream(target), StandardCharsets.UTF_8))) {
                for (String line = reader.readLine(); line != null; line = reader.readLine()) {
                    if (line.isEmpty()) continue;
                    if (earlier.size() == CAPACITY) earlier.removeFirst();
                    earlier.addLast(line);
                }
            } catch (IOException error) { earlier.clear(); }
        }
        // Entries recorded before the file was opened belong after the earlier runs.
        ArrayDeque<String> pending = new ArrayDeque<>(entries);
        entries.clear();
        for (String line : earlier) keep(line);
        for (String line : pending) keep(line);
        rewrite();
    }

    synchronized void append(String entry) {
        keep(entry);
        if (file == null) return;
        // Rewriting from memory once the file holds twice the capacity bounds it without a second file.
        if (writer == null || written >= CAPACITY * 2) { rewrite(); return; }
        try {
            writer.write(entry);
            writer.write('\n');
            writer.flush();
            written++;
        } catch (IOException error) { close(); }
    }

    synchronized String snapshot() {
        return String.join("\n", entries);
    }

    private void keep(String entry) {
        if (entries.size() == CAPACITY) entries.removeFirst();
        entries.addLast(entry);
    }

    private void rewrite() {
        close();
        try {
            File parent = file.getParentFile();
            if (parent != null) parent.mkdirs();
            writer = new OutputStreamWriter(new FileOutputStream(file, false), StandardCharsets.UTF_8);
            for (String line : entries) { writer.write(line); writer.write('\n'); }
            writer.flush();
            written = entries.size();
        } catch (IOException error) { close(); }
    }

    private void close() {
        if (writer == null) return;
        try { writer.close(); } catch (IOException ignored) { }
        writer = null;
    }
}
