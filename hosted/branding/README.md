# Mluva sign-in

The welcome, real email/password form, sign-up, recovery and OAuth return now use Mluva's existing mark and green/neutral palette. The welcome uses readable system type and a compact phone layout, with separate sign-in and account-creation actions. An offline attempt stays on the page with a visible error and usable retry controls. Passwords remain on Cognito's origin; Mluva keeps the existing authorization-code/S256 PKCE flow.

[`settings.json`](settings.json) is the managed-login v2 theme. Logo sources are the existing light/dark medium wordmarks and flat favicon in [`docs/brand`](../../docs/brand/). [`login-branding.py`](../scripts/login-branding.py) reads these files and creates or updates one app client's branding. It checks the AWS account, client membership and existing Essentials/Plus tier. Without `--apply` it only reads and reports hashes. It does not change the pool tier, authentication methods, callbacks or domain version.

[Verification, Impeccable audit and screenshots](../../docs/verification/signin/README.md) cover the real managed form, browser and native callback, failure/recovery and the current native installation. Cognito controls typography and input/button heights; its 32 px controls are a documented limitation. The application's own primary actions are 50 px high. A custom password implementation is outside this change.

## Reproduce

Normal checks run without AWS credentials or networked microphone access:

```sh
cd hosted
npm ci --ignore-scripts
npm run check
npm test
npm run test:browser
```

After authorizing scoped AWS verification, run from the repository root:

```sh
uv run --no-project hosted/scripts/test-login.py --run
```

This creates only a uniquely named temporary Cognito pool/client/domain, migrates its classic domain to v2 with the committed branding, and creates a synthetic user with email suppressed. The browser follows real Cognito OAuth against an in-memory application API peer; credentials travel to the isolated child over stdin and are never saved. The helper removes the test domain, pool, user and branding in `finally`, verifies deletion, and records the result under `tmp/login-evidence/`. The cloud test is deliberately separate from `npm test`. Inspect its receipt before retrying an interrupted run; it refuses to overwrite an outstanding resource receipt. AWS identity usage can be billable. No speech provider, production data or invitations are involved.

Prerequisites: Node.js 24+, Python through uv, Chromium, bubblewrap, Xvfb, D-Bus and authorized managed AWS credentials. The offscreen runner separates display, buses, device access and browser storage; this test retains network access for Cognito HTTPS. Override `MLUVA_OFFSCREEN_RUNNER` if needed. Physical phone keyboards, Safari and native Wayland permission prompts are outside this test.

## Production deployment, pending approval

The audience is the existing public app shell at `https://mluva.danielvecera.com/`; account data remains authenticated. Target: account `762052017888`, region `eu-central-1`, stack `mluva-everywhere-release`, pool `eu-central-1_UcEjScSdb`, client `44uvvv939hl9o7gna9mqssp8q5`, Cognito prefix `mluva-everywhere-762052017888`, S3 bucket `mluva-everywhere-release-site-byzhfneudjgo`, CloudFront `E3HWK9SHYA4M6D`. The existing pool was read-only verified as Essentials; no pricing-tier change is needed. The DNS alias, certificate, callback URLs, provider configuration and private recorder bridge require no changes.

**Prepare.** Save the current CloudFormation original template, stack parameters, Cognito domain version and the six replaced static objects under a private task directory. Run `login-branding.py` without `--apply` against the exact production pool/client above. Prepare the static app with `scripts/configure.mjs` and the current stack outputs; compare its `config.json` byte-for-byte with the live public config. The static change consists of `index.html`, `signin.css`, `callback.html`, `app.mjs`, `auth.mjs` and `auth-callback.mjs`.

**Apply branding after approval.** This operation prepares the inactive v2 theme while the domain still uses classic login:

```sh
uv run --no-project hosted/scripts/login-branding.py \
  --region eu-central-1 --account 762052017888 \
  --user-pool-id eu-central-1_UcEjScSdb \
  --client-id 44uvvv939hl9o7gna9mqssp8q5 --apply
```

**Switch the domain through CloudFormation.** Starting from the saved deployed original template, change only `Resources.LoginDomain.Properties.ManagedLoginVersion` to `2`. Keep every current stack parameter with `UsePreviousValue=true`. Create an UPDATE change set with `CAPABILITY_IAM`; require exactly one resource modification, `LoginDomain`, with no replacement. This preserves deployed Lambda artifact references and avoids an unrelated backend upload. Execute the reviewed change set, wait for `UPDATE_COMPLETE`, and read the domain's version back. Verify the branded login, account creation and password-recovery pages on phone/laptop layouts after Cognito's propagation finishes. The repository's `template.yaml` records the same desired version for subsequent full deployments.

**Publish the prepared six static objects.** Upload only those files to the existing bucket with `Cache-Control: no-cache, max-age=0` and the correct HTML/CSS/JavaScript content types. Do not replace `config.json`, history or existing assets. Invalidate their six paths on CloudFront and wait for completion. Check the canonical URL, the old CloudFront URL, and one authorized synthetic sign-in/sign-out. Recheck `mluva://account`, both web callbacks and native token exchange without invoking the host browser through automated desktop input. No public speech or rewriting is enabled.

**Rollback.** Restore the saved static objects, delete the newly added `signin.css` if rolling back the entire static change, invalidate the same paths, and revert `LoginDomain.ManagedLoginVersion` to `1` through a one-resource CloudFormation change set. Leave the prepared v2 branding record in place for investigation. Retain the current app client, identities, history, callbacks, TLS and DNS. Do not delete or recreate the pool/domain.
