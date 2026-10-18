package io.openaide.android;

import android.content.Context;
import android.util.Log;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.concurrent.Callable;

/**
 * The App Server archive a test build carries, so it can be tried before its
 * version is published. Release builds carry none and download the published one.
 *
 * Termux cannot read this app's files, so the archive is handed over on loopback.
 * Every app on the phone can connect there: the address is unguessable and short
 * lived, and the installer trusts only the digest fixed when the APK was built.
 */
final class BundledRuntime {
    static final String ASSET = "runtime/openaide-app-server-android-arm64.bin";
    private static final int OFFER_MS = 10 * 60_000;

    private BundledRuntime() {}

    static boolean available() { return !BuildConfig.BUNDLED_RUNTIME_SHA256.isEmpty(); }

    /**
     * What the installed runtime's VERSION must say. Test builds of one version
     * differ, so theirs also names the archive and a newer APK replaces the runtime.
     */
    static String expectedVersion() { return expectedVersion(BuildConfig.VERSION_NAME, BuildConfig.BUNDLED_RUNTIME_SHA256); }

    static String expectedVersion(String version, String digest) {
        return digest.isEmpty() ? version : version + "+" + digest.substring(0, 12);
    }

    /** Receives the end of one offer; tests replace the Android log with their own. */
    interface Report { void ended(String outcome, int attempts, long durationMs); }

    /** Starts offering the archive and returns the installer variables that point at it. */
    static String installEnvironment(Context context) throws IOException {
        Context application = context.getApplicationContext();
        return TermuxCommand.variable("OPENAIDE_RUNTIME_URL", offer(() -> application.getAssets().open(ASSET), OFFER_MS, BundledRuntime::log))
            + TermuxCommand.variable("OPENAIDE_RUNTIME_SHA256", BuildConfig.BUNDLED_RUNTIME_SHA256)
            + TermuxCommand.variable("OPENAIDE_RUNTIME_VERSION", expectedVersion());
    }

    /** Offers the archive on loopback for a limited time and returns its address. */
    static String offer(Callable<InputStream> archive, int offerMs, Report report) throws IOException {
        byte[] random = new byte[16];
        new SecureRandom().nextBytes(random);
        StringBuilder token = new StringBuilder();
        for (byte value : random) token.append(String.format("%02x", value));
        ServerSocket server = new ServerSocket(0, 4, InetAddress.getByName("127.0.0.1"));
        String path = "/" + token + "/runtime.tar.gz";
        Thread thread = new Thread(() -> serve(archive, server, path, offerMs, report), "openaide-bundled-runtime");
        thread.setDaemon(true);
        thread.start();
        return "http://127.0.0.1:" + server.getLocalPort() + path;
    }

    /** Serves until one complete transfer or the offer runs out, whichever is first. */
    private static void serve(Callable<InputStream> archive, ServerSocket server, String path, int offerMs, Report report) {
        long started = System.nanoTime() / 1_000_000;
        String outcome = "expired";
        int attempts = 0;
        try (ServerSocket listening = server) {
            while (true) {
                long left = offerMs - (System.nanoTime() / 1_000_000 - started);
                if (left <= 0) break;
                listening.setSoTimeout((int) left);
                try (Socket client = listening.accept()) {
                    attempts++;
                    client.setSoTimeout(30_000);
                    if (send(archive, client, path)) { outcome = "served"; break; }
                } catch (SocketTimeoutException expired) {
                    break;
                } catch (IOException interrupted) {
                    // A dropped transfer is retried by the installer's next request.
                }
            }
        } catch (IOException error) {
            outcome = "failed";
        }
        report.ended(outcome, attempts, System.nanoTime() / 1_000_000 - started);
    }

    private static boolean send(Callable<InputStream> source, Socket client, String path) throws IOException {
        BufferedReader reader = new BufferedReader(new InputStreamReader(client.getInputStream(), StandardCharsets.ISO_8859_1));
        String request = reader.readLine();
        OutputStream output = client.getOutputStream();
        if (!("GET " + path + " HTTP/1.1").equals(request)) {
            output.write("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".getBytes(StandardCharsets.ISO_8859_1));
            return false;
        }
        // Chunked, because a compressed asset does not report its length up front.
        output.write("HTTP/1.1 200 OK\r\nContent-Type: application/gzip\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".getBytes(StandardCharsets.ISO_8859_1));
        try (InputStream archive = open(source)) {
            byte[] buffer = new byte[65536];
            int count;
            while ((count = archive.read(buffer)) != -1) {
                output.write((Integer.toHexString(count) + "\r\n").getBytes(StandardCharsets.ISO_8859_1));
                output.write(buffer, 0, count);
                output.write("\r\n".getBytes(StandardCharsets.ISO_8859_1));
            }
        }
        output.write("0\r\n\r\n".getBytes(StandardCharsets.ISO_8859_1));
        output.flush();
        return true;
    }

    private static InputStream open(Callable<InputStream> source) throws IOException {
        try { return source.call(); }
        catch (IOException error) { throw error; }
        catch (Exception error) { throw new IOException("Archive unavailable"); }
    }

    private static void log(String outcome, int attempts, long durationMs) {
        Log.i("OpenAIDE", "{\"event\":\"bundled_runtime_offer\",\"outcome\":\"" + outcome + "\",\"count\":" + attempts + ",\"duration_ms\":" + durationMs + "}");
    }
}
