package io.openaide.android;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.List;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;

public class DiagnosticsLogTest {
    @Rule public TemporaryFolder folder = new TemporaryFolder();

    private File target() { return new File(new File(folder.getRoot(), "diagnostics"), "phone.jsonl"); }

    private static List<String> lines(File file) throws Exception {
        return Files.readAllLines(file.toPath(), StandardCharsets.UTF_8);
    }

    @Test public void entriesOfAnEarlierRunComeBackBeforeNewOnes() throws Exception {
        DiagnosticsLog first = new DiagnosticsLog();
        first.open(target());
        first.append("one");
        first.append("two");

        DiagnosticsLog second = new DiagnosticsLog();
        second.append("early");
        second.open(target());
        second.append("three");

        assertEquals("one\ntwo\nearly\nthree", second.snapshot());
        assertEquals(List.of("one", "two", "early", "three"), lines(target()));
    }

    @Test public void everyPartOfTheAppSharesOneSnapshot() {
        DiagnosticsLog log = new DiagnosticsLog();
        log.append("settings");
        log.append("connection");

        assertEquals("settings\nconnection", log.snapshot());
    }

    @Test public void keepsOnlyTheNewestEntriesInMemoryAndOnDisk() throws Exception {
        DiagnosticsLog log = new DiagnosticsLog();
        log.open(target());
        int total = DiagnosticsLog.CAPACITY * 3 + 7;
        for (int index = 0; index < total; index++) log.append("entry-" + index);

        String[] kept = log.snapshot().split("\n");
        assertEquals(DiagnosticsLog.CAPACITY, kept.length);
        assertEquals("entry-" + (total - 1), kept[kept.length - 1]);
        assertEquals("entry-" + (total - DiagnosticsLog.CAPACITY), kept[0]);
        List<String> stored = lines(target());
        assertTrue(stored.size() <= DiagnosticsLog.CAPACITY * 2);
        assertEquals("entry-" + (total - 1), stored.get(stored.size() - 1));

        DiagnosticsLog reopened = new DiagnosticsLog();
        reopened.open(target());
        assertEquals(log.snapshot(), reopened.snapshot());
    }

    @Test public void anUnwritableLocationKeepsTheLogInMemory() throws Exception {
        File blocked = folder.newFile("blocked");
        DiagnosticsLog log = new DiagnosticsLog();
        log.open(new File(blocked, "phone.jsonl"));
        log.append("kept");

        assertEquals("kept", log.snapshot());
    }
}
