package io.openaide.android;

import android.util.Base64;
import org.json.JSONObject;
import java.io.IOException;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

final class ServerStatus {
    static final class Incompatible extends IOException {
        Incompatible() { super("Update the OpenAIDE runtime on this server."); }
    }
    final int active;
    final int waiting;
    ServerStatus(int active, int waiting) { this.active = active; this.waiting = waiting; }

    static String authorization(ConnectionProfile profile) {
        return "Basic " + Base64.encodeToString((profile.username + ":" + profile.password)
            .getBytes(StandardCharsets.UTF_8), Base64.NO_WRAP);
    }

    static ServerStatus read(ConnectionProfile profile) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(profile.endpoint + "__openaide-mobile/status").openConnection();
        connection.setConnectTimeout(4000);
        connection.setReadTimeout(4000);
        connection.setInstanceFollowRedirects(false);
        connection.setRequestProperty("Authorization", authorization(profile));
        try {
            int code = connection.getResponseCode();
            if (code == 401 || code == 403) throw new IOException("Credentials rejected. Check connection settings.");
            if (code == 404) throw new Incompatible();
            if (code != 200) throw new IOException("Server unavailable or runtime update required.");
            byte[] body;
            try (var input = connection.getInputStream(); var output = new java.io.ByteArrayOutputStream()) {
                byte[] buffer = new byte[1024];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    output.write(buffer, 0, count);
                    if (output.size() > 4096) break;
                }
                body = output.toByteArray();
            }
            if (body.length > 4096) throw new Incompatible();
            JSONObject status = new JSONObject(new String(body, StandardCharsets.UTF_8));
            if (!"OpenAIDE".equals(status.optString("product")) || status.optInt("mobileProtocol") != 1)
                throw new Incompatible();
            int active = status.getInt("active");
            int waiting = status.getInt("waiting");
            if (active < 0 || waiting < 0) throw new Incompatible();
            return new ServerStatus(active, waiting);
        } catch (org.json.JSONException error) { throw new Incompatible(); }
        finally { connection.disconnect(); }
    }
}
