package io.openaide.android;

import java.io.ByteArrayInputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import org.junit.Test;
import static org.junit.Assert.*;

public class BundledRuntimeTest {
    @Test public void testBuildsOfOneVersionExpectTheirOwnArchive() {
        assertEquals("0.9.0", BundledRuntime.expectedVersion("0.9.0", ""));
        assertEquals("0.9.0+0123456789ab", BundledRuntime.expectedVersion("0.9.0", "0123456789abcdef"));
    }

    @Test public void archiveIsServedOnceToTheAddressItWasOfferedAt() throws Exception {
        byte[] archive = new byte[200_000];
        for (int index = 0; index < archive.length; index++) archive[index] = (byte) (index * 31);
        CompletableFuture<String> ended = new CompletableFuture<>();
        String address = BundledRuntime.offer(() -> new ByteArrayInputStream(archive), 10_000,
            (outcome, attempts, durationMs) -> ended.complete(outcome + ":" + attempts));
        assertTrue(address.matches("http://127\\.0\\.0\\.1:[0-9]+/[0-9a-f]{32}/runtime\\.tar\\.gz"));

        HttpURLConnection guess = (HttpURLConnection) new URL(address.replaceAll("/[0-9a-f]{32}/", "/00/")).openConnection();
        assertEquals(404, guess.getResponseCode());
        guess.disconnect();

        HttpURLConnection request = (HttpURLConnection) new URL(address).openConnection();
        assertEquals(200, request.getResponseCode());
        try (var input = request.getInputStream()) { assertArrayEquals(archive, input.readAllBytes()); }
        assertEquals("served:2", ended.get(5, TimeUnit.SECONDS));
    }

    @Test public void offerEndsByItselfWhenNobodyCollectsIt() throws Exception {
        CompletableFuture<String> ended = new CompletableFuture<>();
        BundledRuntime.offer(() -> new ByteArrayInputStream(new byte[1]), 200, (outcome, attempts, durationMs) -> ended.complete(outcome + ":" + attempts));
        assertEquals("expired:0", ended.get(5, TimeUnit.SECONDS));
    }
}
