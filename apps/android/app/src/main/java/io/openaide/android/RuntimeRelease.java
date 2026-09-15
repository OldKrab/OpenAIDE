package io.openaide.android;

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

    static String installEnvironment() throws Exception {
        String tag = releaseTag(BuildConfig.VERSION_NAME);
        HttpURLConnection request = (HttpURLConnection) new URL(
            "https://api.github.com/repos/OldKrab/OpenAIDE/releases/tags/" + tag).openConnection();
        request.setConnectTimeout(10_000);
        request.setReadTimeout(15_000);
        request.setInstanceFollowRedirects(false);
        request.setRequestProperty("Accept", "application/vnd.github+json");
        try {
            if (request.getResponseCode() != 200) throw new IOException("Release unavailable");
            byte[] response;
            try (var input = request.getInputStream(); var output = new java.io.ByteArrayOutputStream()) {
                byte[] buffer = new byte[4096];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    output.write(buffer, 0, count);
                    if (output.size() > 262144) throw new IOException("Release too large");
                }
                response = output.toByteArray();
            }
            if (response.length > 262144) throw new IOException("Release too large");
            var assets = new JSONObject(new String(response, StandardCharsets.UTF_8)).getJSONArray("assets");
            for (int index = 0; index < assets.length(); index++) {
                var asset = assets.getJSONObject(index);
                if (!"openaide-termux-arm64.tar.gz".equals(asset.optString("name"))) continue;
                String url = asset.getString("browser_download_url");
                String digest = asset.getString("digest");
                if (!url.equals("https://github.com/OldKrab/OpenAIDE/releases/download/" + tag + "/openaide-termux-arm64.tar.gz")
                    || !digest.matches("sha256:[a-f0-9]{64}")) throw new IOException("Unverified release");
                return TermuxCommand.variable("OPENAIDE_RUNTIME_URL", url)
                    + TermuxCommand.variable("OPENAIDE_RUNTIME_SHA256", digest.substring(7));
            }
            throw new IOException("Runtime unavailable");
        } finally { request.disconnect(); }
    }
}
