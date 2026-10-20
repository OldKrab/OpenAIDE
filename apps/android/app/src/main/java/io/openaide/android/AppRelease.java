package io.openaide.android;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Decides whether the newest stable GitHub release updates this app. The release
 * address is fixed here and the APK must be the release's own asset with the
 * digest GitHub computed, so a release response cannot redirect the download.
 */
final class AppRelease {
    static final String LATEST = "https://api.github.com/repos/OldKrab/OpenAIDE/releases/latest";
    static final int MAX_RESPONSE_BYTES = 262144;
    static final long MAX_APK_BYTES = 300L * 1024 * 1024;
    private static final int MAX_NOTES = 20000;
    private static final String STABLE = "\\d+\\.\\d+\\.\\d+";
    private static final String VERSION = STABLE + "(?:-(?:alpha|beta|rc)\\.[1-9]\\d*)?";

    static final class Offer {
        final String version;
        final String notes;
        final String url;
        final String sha256;
        final long sizeBytes;
        final String publishedAt;

        Offer(String version, String notes, String url, String sha256, long sizeBytes, String publishedAt) {
            this.version = version;
            this.notes = notes;
            this.url = url;
            this.sha256 = sha256;
            this.sizeBytes = sizeBytes;
            this.publishedAt = publishedAt;
        }
    }

    /** A release that cannot be trusted or read; `reason` is a stable, metadata-only log value. */
    static final class Rejected extends Exception {
        final String reason;
        Rejected(String reason) { super(reason); this.reason = reason; }
    }

    static String assetName(String version) {
        if (version == null || !version.matches(STABLE)) throw new IllegalArgumentException("Invalid app release version");
        return "openaide-android-" + version + ".apk";
    }

    static String notesUrl(String version) {
        if (version == null || !version.matches(VERSION)) throw new IllegalArgumentException("Invalid app release version");
        return "https://github.com/OldKrab/OpenAIDE/releases/tag/v" + version;
    }

    /** The offer when `release` is a stable version newer than `installed`, otherwise null. */
    static Offer offer(String release, String installed) throws Rejected {
        try {
            JSONObject json = new JSONObject(release);
            if (json.optBoolean("draft") || json.optBoolean("prerelease")) throw new Rejected("not_stable");
            String tag = json.getString("tag_name");
            if (!tag.matches("v" + STABLE)) throw new Rejected("not_stable");
            String version = tag.substring(1);
            if (compare(version, installed) <= 0) return null;
            String name = assetName(version);
            JSONArray assets = json.getJSONArray("assets");
            for (int index = 0; index < assets.length(); index++) {
                JSONObject asset = assets.getJSONObject(index);
                if (!name.equals(asset.optString("name"))) continue;
                String url = asset.getString("browser_download_url");
                String digest = asset.optString("digest");
                long size = asset.getLong("size");
                if (!url.equals("https://github.com/OldKrab/OpenAIDE/releases/download/" + tag + "/" + name)
                    || !digest.matches("sha256:[a-f0-9]{64}")) throw new Rejected("asset_unverified");
                if (size <= 0 || size > MAX_APK_BYTES) throw new Rejected("asset_too_large");
                String notes = json.optString("body");
                if (notes.length() > MAX_NOTES) notes = notes.substring(0, MAX_NOTES);
                String published = json.optString("published_at");
                return new Offer(version, notes, url, digest.substring(7), size, published.isEmpty() ? null : published);
            }
            throw new Rejected("asset_missing");
        } catch (JSONException error) {
            throw new Rejected("malformed_release");
        }
    }

    /** Orders release versions; a prerelease precedes the stable version it leads to. */
    static int compare(String left, String right) {
        if (left == null || !left.matches(VERSION) || right == null || !right.matches(VERSION)) {
            throw new IllegalArgumentException("Invalid app release version");
        }
        String[] a = left.split("-", 2);
        String[] b = right.split("-", 2);
        String[] coreA = a[0].split("\\.");
        String[] coreB = b[0].split("\\.");
        for (int index = 0; index < 3; index++) {
            int order = Long.compare(Long.parseLong(coreA[index]), Long.parseLong(coreB[index]));
            if (order != 0) return order;
        }
        if (a.length == 1 || b.length == 1) return Integer.compare(b.length, a.length);
        String[] preA = a[1].split("\\.");
        String[] preB = b[1].split("\\.");
        int stage = Integer.compare(stage(preA[0]), stage(preB[0]));
        return stage != 0 ? stage : Long.compare(Long.parseLong(preA[1]), Long.parseLong(preB[1]));
    }

    private static int stage(String name) {
        return "alpha".equals(name) ? 0 : "beta".equals(name) ? 1 : 2;
    }
}
