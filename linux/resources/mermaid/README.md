# Mermaid renderer

Mermaid 12.0.0, MIT licensed, rebuilt from its published ESM entrypoint with DOMPurify 3.4.15 and lodash-es 4.18.1. The upstream prebuilt bundle contains an older sanitizer. The overrides address GHSA-55q2-fjhq-7xh7, GHSA-r5fr-rjxr-66jc and GHSA-f23m-r3pf-42rh. The original Mermaid license and bundled dependency license notices are included.

`mermaid.min.js` SHA-256: `6f3bba2204d78ddbee16e6c6fc366f290c7bfe5e718f6482a10365e859e6e40f`.

Rebuild from the repository root with `cd scripts/mermaid && npm ci --ignore-scripts && npm run build`. The lockfile pins every package and its registry integrity hash. `components.json` records the packages actually bundled and the output checksum. Run `npm audit` there and `make linux-fluid-workspace-test` from the repository root after updating.

The app loads this local bundle in an ephemeral WebKitGTK 6.0 view with strict Mermaid security and a restrictive content security policy. No CDN, network access, external images, navigation or persistent browser storage is used. Complete flowcharts, sequence diagrams and other standard Mermaid sketches are supported; diagram configuration directives are rejected. Invalid, unfinished or oversized sketches remain editable source. The source document remains authoritative for Copy, Save and export.
