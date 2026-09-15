package io.openaide.android;

final class SetupPagePolicy {
    static final String ORIGIN = "https://app.openaide.invalid/";

    static String asset(String address) {
        if ((ORIGIN + "index.html").equals(address)) return "setup/index.html";
        if ((ORIGIN + "style.css").equals(address)) return "setup/style.css";
        if ((ORIGIN + "app.js").equals(address)) return "setup/app.js";
        return null;
    }

    static boolean opensSettings(ConnectionProfile profile, String source, String destination, boolean mainFrame, boolean gesture) {
        return mainFrame && gesture && profile.owns(source) && "openaide://connection-settings".equals(destination);
    }

    static boolean isConnectionRoute(ConnectionProfile profile, String address) {
        if (!profile.owns(address)) return false;
        try {
            java.net.URI uri = java.net.URI.create(address);
            if (!"/settings".equals(uri.getPath()) && !"/settings/".equals(uri.getPath())) return false;
            if (uri.getRawQuery() == null) return false;
            int matches = 0;
            for (String parameter : uri.getRawQuery().split("&")) {
                String[] parts = parameter.split("=", 2);
                if (!"tab".equals(java.net.URLDecoder.decode(parts[0], "UTF-8"))) continue;
                if (parts.length != 2 || !"connection".equals(java.net.URLDecoder.decode(parts[1], "UTF-8"))) return false;
                matches++;
            }
            return matches == 1;
        } catch (Exception error) { return false; }
    }
}
