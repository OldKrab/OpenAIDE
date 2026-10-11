package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class TermuxCommandTest {
    @Test public void aDevelopmentBuildKeepsItsOwnFolderInTermux() {
        assertEquals("openaide-android", TermuxCommand.dataName("io.openaide.android"));
        assertEquals("openaide-android-dev", TermuxCommand.dataName("io.openaide.android.dev"));
        assertEquals("openaide-android-dev-two", TermuxCommand.dataName("io.openaide.android.dev.two"));
    }

    @Test public void thePackageNameCannotLeaveTheDataDirectory() {
        assertEquals("openaide-android", TermuxCommand.dataName("other.app/../x"));
        assertEquals("openaide-android-x-x", TermuxCommand.dataName("io.openaide.android./../X$(x)x."));
    }
}
