package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class DownloadNameTest {
    @Test public void prefersTheUnicodeNameTheAppServerSends() {
        assertEquals("сборка build.vsix", DownloadName.from(
            "attachment; filename=\"_______ build.vsix\"; filename*=UTF-8''%D1%81%D0%B1%D0%BE%D1%80%D0%BA%D0%B0%20build.vsix",
            "http://127.0.0.1:5476/__openaide-app-server/download?fileViewerHandle=h"));
        assertEquals("a+b.txt", DownloadName.from("attachment; filename*=UTF-8''a+b.txt", "http://127.0.0.1:5476/x"));
    }

    @Test public void fallsBackToTheQuotedNameThenThePath() {
        assertEquals("report 1.txt", DownloadName.from("attachment; filename=\"report 1.txt\"", "http://127.0.0.1:5476/download"));
        assertEquals("notes.md", DownloadName.from("attachment; filename=notes.md", "http://127.0.0.1:5476/download"));
        assertEquals("check-1", DownloadName.from(null, "http://127.0.0.1:5476/task/check-1"));
        assertEquals("download", DownloadName.from("", "http://127.0.0.1:5476/"));
    }

    @Test public void neverNamesAPathOrAHiddenFile() {
        assertEquals("passwd", DownloadName.from("attachment; filename=\"../../etc/passwd\"", "http://127.0.0.1:5476/"));
        assertEquals("evil.txt", DownloadName.from("attachment; filename*=UTF-8''..%5C..%5Cevil.txt", "http://127.0.0.1:5476/"));
        assertEquals("env", DownloadName.from("attachment; filename=\".env\"", "http://127.0.0.1:5476/"));
        assertEquals("a_b.txt", DownloadName.from("attachment; filename=\"a\u0000b.txt\"", "http://127.0.0.1:5476/"));
        assertEquals("download", DownloadName.from("attachment; filename*=UTF-8''%E0%A4", "http://127.0.0.1:5476/"));
    }

    @Test public void keepsTheExtensionOfALongName() {
        String name = DownloadName.from("attachment; filename=\"" + "x".repeat(300) + ".tar.gz\"", "http://127.0.0.1:5476/");
        assertTrue(name.length() <= 120);
        assertTrue(name.endsWith(".gz"));
    }
}
