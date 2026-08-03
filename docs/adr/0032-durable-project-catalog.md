# Durable Projects are App Server-owned records

Projects are explicit one-root records persisted by an App Server-owned Project Catalog, with stable identity independent of their filesystem path. New Projects receive generated identities, migration preserves legacy Task Project IDs to avoid rewriting history, App Shells register roots through the Catalog, and removal retains the record and Task history; this replaces the previous reconstruction of Projects from Task records, configured roots, and connected editor folders.
