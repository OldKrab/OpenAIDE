package io.openaide.android;

import java.util.Arrays;
import java.util.HashSet;
import java.util.Set;

/**
 * What the connection screen says about a failed connect, and the keys it files
 * "last connected" times under. Kept free of Android so the rules can be tested;
 * the screen reads the phone's network and passes the answer in.
 */
final class ConnectionFailure {
    private ConnectionFailure() {}

    static final String OFFLINE_MESSAGE = "This phone is offline. Connect to the internet, then try again.";

    /** Which sentence the screen shows; the word is also what diagnostics record. */
    enum Classification {
        /** A computer could not be reached and the phone has no working internet. */
        OFFLINE("offline"),
        /** A computer could not be reached while the phone is online. */
        NO_ANSWER("no_answer"),
        /** Any other failure, or this phone's own workspace: the failure's own sentence stands. */
        OTHER("other");

        final String word;
        Classification(String word) { this.word = word; }
    }

    /**
     * Failure classes that mean the computer did not answer: a paired computer's
     * {@code RemoteNode.Failure} and an SSH launch's reason, both lower-cased.
     */
    private static final Set<String> UNREACHABLE = new HashSet<>(Arrays.asList(
        "unreachable", "connection_lost", "timeout", "host_unknown"));

    /**
     * Only a paired or SSH computer can be "offline": this phone's own workspace needs
     * no network. [reason] is the failure class, or null when it is not known.
     */
    static Classification classify(ConnectionProfile.Kind kind, String reason, boolean online) {
        if (kind == ConnectionProfile.Kind.LOCAL || reason == null || !UNREACHABLE.contains(reason)) return Classification.OTHER;
        return online ? Classification.NO_ANSWER : Classification.OFFLINE;
    }

    /** The sentence to show: the offline one, or [message] when the computer is the likelier cause. */
    static String message(Classification classification, String message) {
        return classification == Classification.OFFLINE ? OFFLINE_MESSAGE : message;
    }

    /** Where "last connected" is filed for this phone's own workspace. */
    static final String LOCAL_KEY = "local";

    /**
     * The key a place is filed under, the same one a switch targets: "local" for this
     * phone ([target] null), a paired computer's id, or "ssh:" and an SSH Computer's id.
     */
    static String placeKey(String target) { return target == null ? LOCAL_KEY : target; }

    /** The muted line under a failure, given the relative time as the phone words it. */
    static String lastConnectedLine(String relative) { return "Last connected " + relative; }
}
