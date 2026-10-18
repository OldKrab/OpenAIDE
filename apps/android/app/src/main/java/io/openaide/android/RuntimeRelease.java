package io.openaide.android;

import android.os.SystemClock;
import android.util.Log;
import java.io.IOException;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

final class RuntimeRelease {
    static String releaseTag(String version) {
        if (version == null || !version.matches("\\d+\\.\\d+\\.\\d+(?:-(?:alpha|beta|rc)\\.[1-9]\\d*)?")) {
            throw new IllegalArgumentException("Invalid runtime release version");
        }
        return "v" + version;
    }

    /**
     * The release asset holding the App Server for this phone. An installed app reads only
     * its own version's immutable release, so the name may change between versions.
     */
    static String assetName(String version) {
        releaseTag(version);
        return "openaide-app-server-android-arm64-" + version + ".tar.gz";
    }

    /**
     * Finds this version's runtime in its canonical release. Setup shows every failure as an
     * unavailable download, so only the log tells a missing release from a missing asset.
     */
    static String installEnvironment() throws Exception {
        long started = SystemClock.elapsedRealtime();
        Log.i("OpenAIDE", "runtime_release_lookup outcome=started");
        try {
            String environment = lookUp();
            log("outcome=found", started);
            return environment;
        } catch (Exception error) {
            log("outcome=failed reason=" + (error instanceof LookupFailure ? ((LookupFailure) error).reason
                : error instanceof org.json.JSONException ? "malformed_release"
                : error instanceof IOException ? "network"
                : error instanceof IllegalArgumentException ? "invalid_version" : "error"), started);
            throw error;
        }
    }

    private static String lookUp() throws Exception {
        String tag = releaseTag(BuildConfig.VERSION_NAME);
        String name = assetName(BuildConfig.VERSION_NAME);
        HttpURLConnection request = (HttpURLConnection) new URL(
            "https://api.github.com/repos/OldKrab/OpenAIDE/releases/tags/" + tag).openConnection();
        request.setConnectTimeout(10_000);
        request.setReadTimeout(15_000);
        request.setInstanceFollowRedirects(false);
        request.setRequestProperty("Accept", "application/vnd.github+json");
        try {
            if (request.getResponseCode() != 200) throw new LookupFailure("release_unavailable");
            byte[] response;
            try (var input = request.getInputStream(); var output = new java.io.ByteArrayOutputStream()) {
                byte[] buffer = new byte[4096];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    output.write(buffer, 0, count);
                    if (output.size() > 262144) throw new LookupFailure("release_too_large");
                }
                response = output.toByteArray();
            }
            if (response.length > 262144) throw new LookupFailure("release_too_large");
            var assets = new JSONObject(new String(response, StandardCharsets.UTF_8)).getJSONArray("assets");
            for (int index = 0; index < assets.length(); index++) {
                var asset = assets.getJSONObject(index);
                if (!name.equals(asset.optString("name"))) continue;
                String url = asset.getString("browser_download_url");
                String digest = asset.getString("digest");
                if (!url.equals("https://github.com/OldKrab/OpenAIDE/releases/download/" + tag + "/" + name)
                    || !digest.matches("sha256:[a-f0-9]{64}")) throw new LookupFailure("asset_unverified");
                return TermuxCommand.variable("OPENAIDE_RUNTIME_URL", url)
                    + TermuxCommand.variable("OPENAIDE_RUNTIME_SHA256", digest.substring(7));
            }
            throw new LookupFailure("asset_missing");
        } finally { request.disconnect(); }
    }

    private static void log(String result, long started) {
        Log.i("OpenAIDE", "runtime_release_lookup " + result + " duration_ms=" + (SystemClock.elapsedRealtime() - started));
    }

    /** A lookup that reached GitHub but found no usable runtime; the reason is a fixed code. */
    private static final class LookupFailure extends IOException {
        final String reason;

        LookupFailure(String reason) {
            super(reason);
            this.reason = reason;
        }
    }
}
