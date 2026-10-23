package io.openaide.android;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;

import java.util.List;
import org.junit.Test;

public class SshComputersTest {
    private static final String A = "0123456789abcdef0123456789abcdef";
    private static final String B = "fedcba9876543210fedcba9876543210";
    private static final String KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    private static SshComputers.Computer computer(String id, String name) {
        return new SshComputers.Computer(id, name, "devbox.example", 2222, "dev", KEY);
    }

    @Test public void computersSurviveStorageInOrderAndNamesStayOnOneLine() {
        List<SshComputers.Computer> computers = SshComputers.with(SshComputers.with(SshComputers.parse(""), computer(A, "Build\tbox\nnext")), computer(B, "Lab"));
        List<SshComputers.Computer> stored = SshComputers.parse(SshComputers.format(computers));
        assertEquals(2, stored.size());
        assertEquals("Build box next", stored.get(0).name);
        assertEquals(2222, stored.get(0).port);
        assertEquals(KEY, stored.get(0).hostKey);
        assertEquals(B, stored.get(1).id);
    }

    @Test public void addingAgainReplacesInPlaceAndForgettingRemovesOnlyThatOne() {
        List<SshComputers.Computer> computers = SshComputers.with(SshComputers.with(SshComputers.parse(""), computer(A, "One")), computer(B, "Two"));
        computers = SshComputers.with(computers, computer(A, "Renamed"));
        assertEquals("Renamed", computers.get(0).name);
        assertEquals(1, SshComputers.without(computers, A).size());
        assertNull(SshComputers.find(SshComputers.without(computers, A), A));
    }

    @Test public void storedEntriesThatCouldReachAShellAreDropped() {
        String bad = A + "\tName\tdev box\t22\tdev\t" + KEY + "\n"
            + B + "\tName\thost\t22\tdev;rm\t" + KEY + "\n"
            + A + "\tName\thost\t0\tdev\t" + KEY + "\n"
            + A + "\tName\thost\t22\tdev\tnot a key";
        assertEquals(0, SshComputers.parse(bad).size());
    }

    @Test public void destinationsReadAsTheUserTypesThem() {
        assertArrayEquals(new String[]{"devbox", "22", "dev"}, SshComputers.parseDestination("dev@DevBox", null));
        assertArrayEquals(new String[]{"10.0.0.5", "2222", "dev"}, SshComputers.parseDestination(" dev@10.0.0.5:2222 ", null));
        assertArrayEquals(new String[]{"fe80::1", "2200", "dev"}, SshComputers.parseDestination("dev@[fe80::1]:2200", null));
        assertArrayEquals(new String[]{"fe80::1", "22", "dev"}, SshComputers.parseDestination("dev@fe80::1", null));
        assertNull(SshComputers.parseDestination("devbox", null));
        assertNull(SshComputers.parseDestination("dev@host:99999", null));
        assertNull(SshComputers.parseDestination("dev@-oProxyCommand=x", null));
        assertNull(SshComputers.parseDestination("dev@host name", null));
    }

    @Test public void theDestinationShownNamesAPortOnlyWhenItIsNotTheDefault() {
        assertEquals("dev@devbox.example:2222", computer(A, "x").destination());
        assertEquals("dev@[fe80::1]", new SshComputers.Computer(A, "x", "fe80::1", 22, "dev", KEY).destination());
    }
}
