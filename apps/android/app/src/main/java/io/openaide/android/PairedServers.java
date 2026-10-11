package io.openaide.android;

import java.util.ArrayList;
import java.util.List;

/**
 * The computers this phone is paired with, as stored text: one per line, the
 * key, a tab, then the name. Kept free of Android so the rules can be tested.
 */
final class PairedServers {
    private PairedServers() {}

    static List<ConnectionStore.PairedServer> parse(String stored) {
        List<ConnectionStore.PairedServer> servers = new ArrayList<>();
        for (String line : stored.split("\n")) {
            int separator = line.indexOf('\t');
            String id = separator < 0 ? line : line.substring(0, separator);
            if (!validId(id) || indexOf(servers, id) >= 0) continue;
            servers.add(new ConnectionStore.PairedServer(id, separator < 0 ? "" : line.substring(separator + 1)));
        }
        return servers;
    }

    static String format(List<ConnectionStore.PairedServer> servers) {
        StringBuilder stored = new StringBuilder();
        for (ConnectionStore.PairedServer server : servers) {
            if (stored.length() > 0) stored.append('\n');
            // The name comes from the computer; a line break in it must not become another entry.
            stored.append(server.id).append('\t').append(server.name.replaceAll("[\\p{Cntrl}]", " "));
        }
        return stored.toString();
    }

    /** Pairing again keeps the computer's place in the list and takes its new name. */
    static List<ConnectionStore.PairedServer> with(List<ConnectionStore.PairedServer> servers, ConnectionStore.PairedServer server) {
        List<ConnectionStore.PairedServer> next = new ArrayList<>(servers);
        int index = indexOf(next, server.id);
        if (index >= 0) next.set(index, server); else next.add(server);
        return next;
    }

    static List<ConnectionStore.PairedServer> without(List<ConnectionStore.PairedServer> servers, String id) {
        List<ConnectionStore.PairedServer> next = new ArrayList<>(servers);
        int index = indexOf(next, id);
        if (index >= 0) next.remove(index);
        return next;
    }

    static ConnectionStore.PairedServer find(List<ConnectionStore.PairedServer> servers, String id) {
        int index = indexOf(servers, id);
        return index < 0 ? null : servers.get(index);
    }

    static boolean validId(String id) { return id != null && id.matches("[0-9a-f]{64}"); }

    private static int indexOf(List<ConnectionStore.PairedServer> servers, String id) {
        for (int index = 0; index < servers.size(); index++) if (servers.get(index).id.equals(id)) return index;
        return -1;
    }
}
