# Frozen first-character titlecase

`titlecase-first.json` contains the 135 Unicode 16.0.0 characters whose full one-character titlecase differs from uppercase. The reference runtime's Unicode version is checked during temporary collection; SHA-256 is `611d65656ef1a4447f35c85c811077b8dc0a0041b2d91d11a29154531d49dd11`.

Rewrite thinking labels combine these overrides with the native core's frozen lower/upper mappings. This preserves expansions, titlecase digraphs and context-sensitive final sigma without an interpreter or reliance on the compiler's Unicode version. Independently observed released GTK labels are recorded in `tests/fixtures/released-capture-controls.json`.
