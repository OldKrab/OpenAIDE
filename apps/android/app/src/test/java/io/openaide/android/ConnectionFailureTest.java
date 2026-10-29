package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class ConnectionFailureTest {
    private static final ConnectionProfile.Kind PAIRED = ConnectionProfile.Kind.PAIRED;
    private static final ConnectionProfile.Kind SSH = ConnectionProfile.Kind.SSH;

    @Test public void anUnreachableComputerWithoutInternetSaysThePhoneIsOffline() {
        for (ConnectionProfile.Kind kind : new ConnectionProfile.Kind[]{PAIRED, SSH}) {
            ConnectionFailure.Classification offline = ConnectionFailure.classify(kind, "unreachable", false);
            assertEquals(ConnectionFailure.Classification.OFFLINE, offline);
            assertEquals(ConnectionFailure.OFFLINE_MESSAGE, ConnectionFailure.message(offline, "Can’t reach Desk."));
        }
    }

    @Test public void anUnreachableComputerWithInternetKeepsTheSentenceThatNamesIt() {
        ConnectionFailure.Classification noAnswer = ConnectionFailure.classify(PAIRED, "unreachable", true);
        assertEquals(ConnectionFailure.Classification.NO_ANSWER, noAnswer);
        assertEquals("Can’t reach Desk.", ConnectionFailure.message(noAnswer, "Can’t reach Desk."));
    }

    @Test public void sshLaunchFailuresThatMeanNoAnswerAreClassifiedLikeAPairedOne() {
        for (String reason : new String[]{"unreachable", "connection_lost", "timeout", "host_unknown"}) {
            assertEquals(reason, ConnectionFailure.Classification.OFFLINE, ConnectionFailure.classify(SSH, reason, false));
            assertEquals(reason, ConnectionFailure.Classification.NO_ANSWER, ConnectionFailure.classify(SSH, reason, true));
        }
    }

    @Test public void aFailureThatIsNotAMissingAnswerIsNeverCalledOffline() {
        for (String reason : new String[]{"not_trusted", "incompatible", "storage", "auth_failed", "host_key_changed", "update_required"}) {
            ConnectionFailure.Classification other = ConnectionFailure.classify(PAIRED, reason, false);
            assertEquals(reason, ConnectionFailure.Classification.OTHER, other);
            assertEquals("Its own words.", ConnectionFailure.message(other, "Its own words."));
        }
        assertEquals(ConnectionFailure.Classification.OTHER, ConnectionFailure.classify(PAIRED, null, false));
    }

    @Test public void thisPhonesOwnWorkspaceNeedsNoNetwork() {
        assertEquals(ConnectionFailure.Classification.OTHER,
            ConnectionFailure.classify(ConnectionProfile.Kind.LOCAL, "unreachable", false));
    }

    @Test public void lastConnectedIsFiledUnderTheSwitchTarget() {
        assertEquals("local", ConnectionFailure.computerKey(null));
        assertEquals("a".repeat(64), ConnectionFailure.computerKey("a".repeat(64)));
        assertEquals("ssh:build-1", ConnectionFailure.computerKey(ComputerSwitch.SSH_PREFIX + "build-1"));
    }

    @Test public void theLastConnectedLineReadsAsASentence() {
        assertEquals("Last connected 5 minutes ago", ConnectionFailure.lastConnectedLine("5 minutes ago"));
    }
}
