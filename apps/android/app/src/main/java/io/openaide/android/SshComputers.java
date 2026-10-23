package io.openaide.android;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * The SSH Computers this phone knows (ADR-0069), as stored text: one per line,
 * tab-separated id, name, host, port, user, and the host key the user accepted.
 * Kept free of Android so the rules can be tested.
 */
final class SshComputers {
    private SshComputers() {}

    /** An SSH Computer as the user added it. The host key is public; nothing here is secret. */
    static final class Computer {
        final String id;
        final String name;
        final String host;
        final int port;
        final String user;
        /** The OpenSSH host key accepted on first connection; a different one blocks the connection. */
        final String hostKey;

        Computer(String id, String name, String host, int port, String user, String hostKey) {
            this.id = id;
            this.name = name;
            this.host = host;
            this.port = port;
            this.user = user;
            this.hostKey = hostKey;
        }

        /** `user@host`, with the port only when it is not 22, as the user would type it. */
        String destination() {
            String address = host.indexOf(':') >= 0 ? "[" + host + "]" : host;
            return user + "@" + address + (port == 22 ? "" : ":" + port);
        }
    }

    static List<Computer> parse(String stored) {
        List<Computer> computers = new ArrayList<>();
        for (String line : stored.split("\n")) {
            String[] fields = line.split("\t", -1);
            if (fields.length != 6) continue;
            int port;
            try { port = Integer.parseInt(fields[3]); } catch (NumberFormatException error) { continue; }
            if (!validId(fields[0]) || !validHost(fields[2]) || !validPort(port) || !validUser(fields[4])
                || !validHostKey(fields[5]) || find(computers, fields[0]) != null) continue;
            computers.add(new Computer(fields[0], fields[1], fields[2], port, fields[4], fields[5]));
        }
        return computers;
    }

    static String format(List<Computer> computers) {
        StringBuilder stored = new StringBuilder();
        for (Computer computer : computers) {
            if (stored.length() > 0) stored.append('\n');
            stored.append(computer.id).append('\t').append(name(computer.name)).append('\t').append(computer.host)
                .append('\t').append(computer.port).append('\t').append(computer.user).append('\t').append(computer.hostKey);
        }
        return stored.toString();
    }

    /** Adding a computer again with the same id replaces it in place. */
    static List<Computer> with(List<Computer> computers, Computer computer) {
        List<Computer> next = without(computers, computer.id);
        int index = indexOf(computers, computer.id);
        next.add(index < 0 ? next.size() : index, computer);
        return next;
    }

    static List<Computer> without(List<Computer> computers, String id) {
        List<Computer> next = new ArrayList<>(computers);
        int index = indexOf(next, id);
        if (index >= 0) next.remove(index);
        return next;
    }

    static Computer find(List<Computer> computers, String id) {
        int index = indexOf(computers, id);
        return index < 0 ? null : computers.get(index);
    }

    /** A name the user typed, on one line and of a length Settings can show. */
    static String name(String typed) {
        String name = typed == null ? "" : typed.replaceAll("[\\p{Cntrl}]", " ").trim();
        return name.length() > 64 ? name.substring(0, 64) : name;
    }

    static boolean validId(String id) { return id != null && id.matches("[0-9a-f]{32}"); }

    /** A DNS name or an IP address; nothing a shell or URL could read as more. */
    static boolean validHost(String host) {
        return host != null && host.length() <= 253 && host.matches("[A-Za-z0-9._:%-]+") && !host.startsWith("-");
    }

    /** A Linux user name as `useradd` accepts it. */
    static boolean validUser(String user) {
        return user != null && user.length() <= 64 && user.matches("[A-Za-z0-9._][A-Za-z0-9._-]*\\$?");
    }

    static boolean validPort(int port) { return port > 0 && port < 65536; }

    /** One OpenSSH public key line, as the SSH library returns it. */
    static boolean validHostKey(String key) {
        return key != null && key.length() < 16384 && key.matches("(ssh|ecdsa)-[A-Za-z0-9@.-]+ [A-Za-z0-9+/=]+( [^\\t\\n]*)?");
    }

    /**
     * Reads what the user typed in Host: `host`, `user@host`, `host:port`, or
     * `[v6]:port`. Returns null for anything else.
     */
    static String[] parseDestination(String typed, String defaultUser) {
        String text = typed == null ? "" : typed.trim();
        String user = defaultUser == null ? "" : defaultUser.trim();
        int at = text.lastIndexOf('@');
        if (at >= 0) { user = text.substring(0, at); text = text.substring(at + 1); }
        String host = text;
        int port = 22;
        if (text.startsWith("[")) {
            int end = text.indexOf(']');
            if (end < 0) return null;
            host = text.substring(1, end);
            String rest = text.substring(end + 1);
            if (!rest.isEmpty()) {
                if (!rest.startsWith(":")) return null;
                port = port(rest.substring(1));
            }
        } else if (text.indexOf(':') >= 0 && text.indexOf(':') == text.lastIndexOf(':')) {
            host = text.substring(0, text.indexOf(':'));
            port = port(text.substring(text.indexOf(':') + 1));
        }
        host = host.toLowerCase(Locale.ROOT);
        if (!validHost(host) || !validUser(user) || !validPort(port)) return null;
        return new String[]{host, Integer.toString(port), user};
    }

    private static int port(String text) {
        try { return Integer.parseInt(text); } catch (NumberFormatException error) { return -1; }
    }

    private static int indexOf(List<Computer> computers, String id) {
        for (int index = 0; index < computers.size(); index++) if (computers.get(index).id.equals(id)) return index;
        return -1;
    }
}
