package io.openaide.android;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import org.junit.Test;
import static org.junit.Assert.*;

public class SwitchChoicesTest {
    private static final String A = "a".repeat(64);
    private static final String B = "b".repeat(64);
    private static final List<ConnectionStore.PairedServer> PAIRED = Arrays.asList(
        new ConnectionStore.PairedServer(A, "Desk"), new ConnectionStore.PairedServer(B, "Laptop"));
    private static final List<SshComputers.Computer> SSH = Collections.singletonList(
        new SshComputers.Computer("build", "Build box", "host.example", 22, "dev", "key"));

    private static List<String> labels(List<SwitchChoices.Choice> choices) {
        List<String> labels = new ArrayList<>();
        for (SwitchChoices.Choice choice : choices) labels.add(choice.label());
        return labels;
    }

    @Test public void thisPhoneIsOfferedFirstWhenReadyAndEveryOtherComputerFollows() {
        assertEquals(Arrays.asList("Use this phone", "Use Desk", "Use Laptop", "Use Build box"),
            labels(SwitchChoices.list(true, PAIRED, SSH, ComputerSwitch.SSH_PREFIX + "gone")));
    }

    @Test public void thisPhoneIsLeftOutUntilItsWorkspaceIsReady() {
        assertEquals(Arrays.asList("Use Desk", "Use Laptop", "Use Build box"),
            labels(SwitchChoices.list(false, PAIRED, SSH, ComputerSwitch.SSH_PREFIX + "gone")));
    }

    @Test public void thePairedComputerInUseIsLeftOut() {
        assertEquals(Arrays.asList("Use this phone", "Use Laptop", "Use Build box"), labels(SwitchChoices.list(true, PAIRED, SSH, A)));
    }

    @Test public void theSshComputerInUseIsLeftOut() {
        assertEquals(Arrays.asList("Use this phone", "Use Desk", "Use Laptop"),
            labels(SwitchChoices.list(true, PAIRED, SSH, ComputerSwitch.SSH_PREFIX + "build")));
    }

    @Test public void thisPhoneInUseIsLeftOutEvenWhenReady() {
        assertEquals(Arrays.asList("Use Desk", "Use Laptop", "Use Build box"), labels(SwitchChoices.list(true, PAIRED, SSH, null)));
    }

    @Test public void eachChoiceTargetsWhatTheSwitchExpects() {
        List<SwitchChoices.Choice> choices = SwitchChoices.list(true, PAIRED, SSH, ComputerSwitch.SSH_PREFIX + "gone");
        assertNull(choices.get(0).target);
        assertEquals(A, choices.get(1).target);
        assertEquals("ssh:build", choices.get(3).target);
        assertEquals("Connecting to this phone…", choices.get(0).connectingText());
        assertEquals("Connecting to Build box…", choices.get(3).connectingText());
    }

    @Test public void nothingIsOfferedWhenThereIsNowhereElse() {
        assertTrue(SwitchChoices.list(false, Collections.singletonList(PAIRED.get(0)), Collections.emptyList(), A).isEmpty());
    }

    @Test public void aComputerWithoutANameIsStillOffered() {
        List<SwitchChoices.Choice> choices = SwitchChoices.list(false,
            Collections.singletonList(new ConnectionStore.PairedServer(A, " ")), Collections.emptyList(), null);
        assertEquals("Use an unnamed computer", choices.get(0).label());
    }
}
