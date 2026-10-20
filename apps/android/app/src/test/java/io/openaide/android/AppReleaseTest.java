package io.openaide.android;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;

public class AppReleaseTest {
    private static final String DIGEST = "sha256:" + "ab".repeat(32);

    private static String release(String tag, String assetName, String url, String digest, long size) throws Exception {
        return new JSONObject().put("tag_name", tag).put("draft", false).put("prerelease", false)
            .put("body", "## Faster").put("published_at", "2026-10-01T00:00:00Z")
            .put("assets", new JSONArray().put(new JSONObject().put("name", assetName)
                .put("browser_download_url", url).put("digest", digest).put("size", size))).toString();
    }

    private static String release(String version) throws Exception {
        String name = "openaide-android-" + version + ".apk";
        return release("v" + version, name, "https://github.com/OldKrab/OpenAIDE/releases/download/v" + version + "/" + name, DIGEST, 1024);
    }

    @Test public void offersANewerStableReleaseApk() throws Exception {
        AppRelease.Offer offer = AppRelease.offer(release("0.11.0"), "0.10.0");
        assertEquals("0.11.0", offer.version);
        assertEquals("ab".repeat(32), offer.sha256);
        assertEquals(1024, offer.sizeBytes);
        assertEquals("## Faster", offer.notes);
    }

    @Test public void offersNothingForTheInstalledOrAnOlderVersion() throws Exception {
        assertNull(AppRelease.offer(release("0.10.0"), "0.10.0"));
        assertNull(AppRelease.offer(release("0.9.9"), "0.10.0"));
    }

    @Test public void stableReleaseUpdatesItsOwnPrerelease() throws Exception {
        assertNotNull(AppRelease.offer(release("0.10.0"), "0.10.0-rc.2"));
        assertTrue(AppRelease.compare("0.10.0-beta.3", "0.10.0-rc.1") < 0);
        assertTrue(AppRelease.compare("0.10.0-rc.10", "0.10.0-rc.9") > 0);
        assertTrue(AppRelease.compare("0.10.0", "0.9.99") > 0);
    }

    @Test public void rejectsPrereleasesAndForeignOrUnverifiedAssets() throws Exception {
        String name = "openaide-android-0.11.0.apk";
        String canonical = "https://github.com/OldKrab/OpenAIDE/releases/download/v0.11.0/" + name;
        assertRejected("not_stable", release("v0.11.0-rc.1", "openaide-android-0.11.0-rc.1.apk", canonical, DIGEST, 1024));
        assertRejected("asset_unverified", release("v0.11.0", name, "https://example.com/" + name, DIGEST, 1024));
        assertRejected("asset_unverified", release("v0.11.0", name, canonical, "sha1:abc", 1024));
        assertRejected("asset_missing", release("v0.11.0", "openaide-android.apk", canonical, DIGEST, 1024));
        assertRejected("asset_too_large", release("v0.11.0", name, canonical, DIGEST, AppRelease.MAX_APK_BYTES + 1));
        assertRejected("malformed_release", "{}");
        assertRejected("not_stable", new JSONObject(release("0.11.0")).put("prerelease", true).toString());
    }

    @Test public void releaseNotesStayOnTheProjectReleasePage() {
        assertEquals("https://github.com/OldKrab/OpenAIDE/releases/tag/v0.11.0", AppRelease.notesUrl("0.11.0"));
        assertThrows(IllegalArgumentException.class, () -> AppRelease.notesUrl("0.11.0/../../evil"));
    }

    private static void assertRejected(String reason, String release) {
        AppRelease.Rejected rejected = assertThrows(AppRelease.Rejected.class, () -> AppRelease.offer(release, "0.10.0"));
        assertEquals(reason, rejected.reason);
    }
}
