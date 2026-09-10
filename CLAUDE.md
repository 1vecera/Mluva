# Mluva repository guide

Start with the [code map and focused checks](CONTRIBUTING.md#code-map). It maps the runtime modules, isolated GUI runners and required checks. The [product contract](docs/product-contract.md) and [Linux platform profile](docs/linux-platform-profile.md) define behavior and platform limits. Omarchy is primary; Fedora GNOME compatibility has not been tested recently.

- Use `make linux-setup` for the locked Linux environment with distribution PyGObject access.
- Use `make linux-test-fast` for text/editing feedback and `make linux-test` for all Linux tests, Ruff and generated-feature consistency. Follow the contribution guide for native integration checks.
- Run GUI acceptance only through isolated offscreen runners; never drive the active desktop.
- Keep capture and delivery independent of provider choice. PCM is signed little-endian, 16 kHz, 16-bit, mono.
- Keep raw recognition immutable and separate from working edits, processed text and delivered text. Never deliver volatile recognition.
- Preserve session/revision checks around asynchronous results and exact-target checks around insertion. Clipboard-only delivery remains available when insertion permission is absent.
- Honor retention policy; Incognito must write neither history nor retained audio. Never persist or log credentials or provider secrets.
