package io.openaide.android;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * The computers the connection screen offers when the computer in use cannot be used
 * (ADR-0070): every other computer the user can work on. Kept free of Android so the
 * rules can be tested.
 */
final class SwitchChoices {
    private SwitchChoices() {}

    /** One computer to switch to. [target] is a {@link ComputerSwitch} target: null for this phone. */
    static final class Choice {
        final String target;
        /** Names the computer in a sentence: "this phone" or the computer's name. */
        final String name;

        Choice(String target, String name) { this.target = target; this.name = name; }

        String label() { return "Use " + name; }
        String connectingText() { return "Connecting to " + name + "…"; }
    }

    /**
     * Every computer but [inUse] (null for this phone): this phone when its workspace is
     * [thisPhoneReady], then the paired computers, then the SSH Computers, each in the
     * order the phone keeps them.
     */
    static List<Choice> list(boolean thisPhoneReady, List<ConnectionStore.PairedServer> paired,
                             List<SshComputers.Computer> ssh, String inUse) {
        List<Choice> choices = new ArrayList<>();
        if (thisPhoneReady && inUse != null) choices.add(new Choice(null, "this phone"));
        for (ConnectionStore.PairedServer server : paired) {
            if (!server.id.equals(inUse)) choices.add(new Choice(server.id, named(server.name)));
        }
        for (SshComputers.Computer computer : ssh) {
            String target = ComputerSwitch.SSH_PREFIX + computer.id;
            if (!target.equals(inUse)) choices.add(new Choice(target, named(computer.name)));
        }
        return Collections.unmodifiableList(choices);
    }

    private static String named(String name) { return name == null || name.trim().isEmpty() ? "an unnamed computer" : name; }
}
