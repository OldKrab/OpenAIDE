package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class RuntimeReleaseTest {
    @Test public void runtimeIsPinnedToTheApkVersion() {
        assertEquals("v0.5.5", RuntimeRelease.releaseTag("0.5.5"));
        assertEquals("v0.6.0-rc.2", RuntimeRelease.releaseTag("0.6.0-rc.2"));
        assertEquals("v" + BuildConfig.VERSION_NAME, RuntimeRelease.releaseTag(BuildConfig.VERSION_NAME));
        assertTrue(BuildConfig.VERSION_CODE > 7);
    }

    @Test public void versionCannotChangeTheReleaseOriginOrPath() {
        for (String version : new String[]{null, "", "v0.5.5", "0.5.5/../latest", "0.5.5?other=1", "0.5.5\n", "0.5.5-rc.0"}) {
            assertThrows(IllegalArgumentException.class, () -> RuntimeRelease.releaseTag(version));
        }
    }
}
