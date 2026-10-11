package io.openaide.android;

import java.util.List;
import org.junit.Test;
import static org.junit.Assert.*;

public class PairedServersTest {
    private static final String A = "a".repeat(64);
    private static final String B = "b".repeat(64);

    private static ConnectionStore.PairedServer server(String id, String name) { return new ConnectionStore.PairedServer(id, name); }

    @Test public void pairingAnotherComputerKeepsTheFirst() {
        List<ConnectionStore.PairedServer> servers = PairedServers.with(PairedServers.with(PairedServers.parse(""), server(A, "Desk")), server(B, "Laptop"));
        List<ConnectionStore.PairedServer> stored = PairedServers.parse(PairedServers.format(servers));
        assertEquals(2, stored.size());
        assertEquals("Desk", stored.get(0).name);
        assertEquals("Laptop", PairedServers.find(stored, B).name);
    }

    @Test public void pairingAgainRenamesInPlaceAndForgettingRemovesOnlyThatComputer() {
        List<ConnectionStore.PairedServer> servers = PairedServers.with(PairedServers.with(PairedServers.parse(""), server(A, "Desk")), server(B, "Laptop"));
        servers = PairedServers.with(servers, server(A, "Studio"));
        assertEquals(2, servers.size());
        assertEquals("Studio", servers.get(0).name);
        servers = PairedServers.without(servers, A);
        assertEquals(1, servers.size());
        assertNull(PairedServers.find(servers, A));
        assertEquals(1, PairedServers.without(servers, A).size());
    }

    @Test public void aNameCannotAddAnEntryAndDamagedLinesAreDropped() {
        String stored = PairedServers.format(List.of(server(A, "Desk\n" + B + "\tIntruder")));
        assertEquals(1, PairedServers.parse(stored).size());
        assertEquals(0, PairedServers.parse("not-a-key\tDesk\n\n").size());
        assertEquals(1, PairedServers.parse(A + "\tDesk\n" + A + "\tAgain").size());
        assertEquals("", PairedServers.parse(A).get(0).name);
    }
}
