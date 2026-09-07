# Removed Surface Inventory

The retained baseline contained the deleted subsystem in these cohorts. Each cohort is closed only when both implementation references and named residue are absent.

| Cohort | Disposition |
|---|---|
| Product commands, automation, agent tools, and permissions | Delete |
| Rust domain, application, transitions, projections, and facade | Delete |
| Rust persistence tables, outbox paths, receipts, and tests | Delete; add neutral schema deletion migration |
| Swift client, composition, identity, and receipt translation | Delete |
| Tuist package, dependency locks, release inputs, and preparation scripts | Delete or regenerate |
| Swift and Kotlin generated bindings and fixtures | Regenerate from reduced facade |
| Dedicated architecture, product, wiki, and planning records | Delete |
| Mixed current and historical records | Remove subsystem-specific content |
| Repository paths and textual identifiers | Enforce literal zero with self-tested ratchet |

Closure requires a clean path/content scan, clean regenerated build artifacts, green architecture checks, complete platform tests, and a usable post-launch simulator accessibility tree.
