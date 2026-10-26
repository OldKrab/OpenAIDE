package io.openaide.android;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;
import static org.junit.Assert.*;

public class DiagnosticsShareTest {
    @Rule public TemporaryFolder temp = new TemporaryFolder();

    @Test public void eachShareReplacesTheNamedFileWithTheCurrentSnapshot() throws Exception {
        File directory = new File(temp.getRoot(), DiagnosticsShare.DIRECTORY);

        DiagnosticsShare.write(directory, "{\"event\":\"a\"}\n{\"event\":\"b\"}");
        File file = DiagnosticsShare.write(directory, "{\"event\":\"c\"}");

        assertEquals("openaide-phone-diagnostics.jsonl", file.getName());
        assertEquals("{\"event\":\"c\"}\n", new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8));
    }

    @Test public void anEmptySnapshotSharesAnEmptyFile() throws Exception {
        File file = DiagnosticsShare.write(temp.getRoot(), "");
        assertEquals(0, file.length());
    }
}
