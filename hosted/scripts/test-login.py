# /// script
# requires-python = ">=3.13"
# dependencies = ["boto3>=1.40"]
# ///
"""Verify the real managed login in a disposable Cognito pool, then remove it."""

import argparse
import json
import os
import secrets
import subprocess
import sys
from pathlib import Path

import boto3


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--run",
        action="store_true",
        help="Create temporary AWS identity resources and run the isolated browser.",
    )
    parser.add_argument("--region", default="eu-central-1")
    parser.add_argument("--account", default="762052017888")
    args = parser.parse_args()
    if not args.run:
        print(
            "With --run: create an isolated Cognito Essentials pool, client, domain, branding and suppressed synthetic user; verify OAuth in Chromium; delete the pool and domain. No production writes, email or speech. AWS identity usage can be billable."
        )
        return
    if (
        boto3.client("sts", region_name=args.region).get_caller_identity()["Account"]
        != args.account
    ):
        parser.error("Unexpected AWS account; nothing changed.")
    root = Path(__file__).resolve().parents[2]
    os.chdir(root)
    evidence = root / "tmp/login-evidence"
    evidence.mkdir(parents=True, exist_ok=True)
    resources = evidence / "resources.json"
    if resources.exists() and not json.loads(resources.read_text()).get("deleted"):
        parser.error("A previous resource receipt needs cleanup before another run.")
    runner = Path(
        os.environ.get(
            "MLUVA_OFFSCREEN_RUNNER",
            str(
                Path.home()
                / ".agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh"
            ),
        )
    )
    if not runner.is_file():
        runner = root / "dev/run-isolated.sh"
    client = boto3.client("cognito-idp", region_name=args.region)
    domain = f"mluva-signin-test-{secrets.token_hex(6)}"
    receipt = {"account": args.account, "region": args.region, "domain": domain}
    pool = None
    domain_created = False
    try:
        pool = client.create_user_pool(
            PoolName=domain,
            UserPoolTier="ESSENTIALS",
            UsernameAttributes=["email"],
            AutoVerifiedAttributes=["email"],
            Policies={
                "PasswordPolicy": {
                    "MinimumLength": 12,
                    "RequireUppercase": True,
                    "RequireLowercase": True,
                    "RequireNumbers": True,
                    "RequireSymbols": True,
                }
            },
            UserPoolTags={
                "project": "Mluva",
                "purpose": "isolated-signin-verification",
                "temporary": "true",
            },
        )["UserPool"]["Id"]
        receipt["pool"] = pool
        resources.write_text(json.dumps(receipt, indent=2))
        app = client.create_user_pool_client(
            UserPoolId=pool,
            ClientName="isolated-signin-verification",
            GenerateSecret=False,
            AllowedOAuthFlowsUserPoolClient=True,
            AllowedOAuthFlows=["code"],
            AllowedOAuthScopes=["openid", "email"],
            SupportedIdentityProviders=["COGNITO"],
            CallbackURLs=["http://127.0.0.1:8792/callback.html", "mluva://account"],
            LogoutURLs=["http://127.0.0.1:8792/"],
            PreventUserExistenceErrors="ENABLED",
            EnableTokenRevocation=True,
        )["UserPoolClient"]["ClientId"]
        receipt["client"] = app
        resources.write_text(json.dumps(receipt, indent=2))
        client.create_user_pool_domain(
            UserPoolId=pool, Domain=domain, ManagedLoginVersion=1
        )
        domain_created = True
        # This helper uses the parent's AWS credentials; none enter the GUI child.
        with (evidence / "branding.json").open("w") as report:
            subprocess.run(
                [
                    sys.executable,
                    "hosted/scripts/login-branding.py",
                    "--region",
                    args.region,
                    "--account",
                    args.account,
                    "--user-pool-id",
                    pool,
                    "--client-id",
                    app,
                    "--apply",
                ],
                stdout=report,
                check=True,
            )
        client.update_user_pool_domain(
            UserPoolId=pool, Domain=domain, ManagedLoginVersion=2
        )
        email = f"signin-{secrets.token_hex(5)}@example.invalid"
        password = secrets.token_urlsafe(32) + "aA1!"
        client.admin_create_user(
            UserPoolId=pool,
            Username=email,
            MessageAction="SUPPRESS",
            UserAttributes=[
                {"Name": "email", "Value": email},
                {"Name": "email_verified", "Value": "true"},
            ],
        )
        client.admin_set_user_password(
            UserPoolId=pool, Username=email, Password=password, Permanent=True
        )
        environment = {
            "PATH": os.environ["PATH"],
            "HOME": os.environ["HOME"],
            "LC_ALL": "C.UTF-8",
            "OFFSCREEN_DISPLAY_NUMBER": os.environ.get(
                "OFFSCREEN_DISPLAY_NUMBER", "295"
            ),
            "__EGL_VENDOR_LIBRARY_FILENAMES": "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            "__GLX_VENDOR_LIBRARY_NAME": "mesa",
            "LIBGL_ALWAYS_SOFTWARE": "1",
            "GALLIUM_DRIVER": "llvmpipe",
        }
        # Network is needed only for Cognito HTTPS. Devices, display, buses,
        # browser storage and environment remain separate from the desktop.
        command = [
            "bwrap",
            "--unshare-pid",
            "--die-with-parent",
            "--bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--tmpfs",
            "/dev/shm",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/run/dbus",
            "--tmpfs",
            "/usr/share/dbus-1/services",
            "--proc",
            "/proc",
            "--",
            "bash",
            str(runner),
            "tmp/login-evidence/browser",
            "--",
            "node",
            "hosted/tests/login-browser.mjs",
        ]
        result = subprocess.run(
            command,
            env=environment,
            input=json.dumps({"email": email, "password": password}),
            text=True,
            check=False,
            timeout=300,
        )
        receipt["browserExit"] = result.returncode
        if result.returncode:
            raise RuntimeError(
                "Managed-login browser verification failed; see its phase above."
            )
    finally:
        if domain_created:
            client.delete_user_pool_domain(UserPoolId=pool, Domain=domain)
        if pool:
            client.delete_user_pool(UserPoolId=pool)
            try:
                client.describe_user_pool(UserPoolId=pool)
            except client.exceptions.ResourceNotFoundException:
                receipt["deleted"] = True
            resources.write_text(json.dumps(receipt, indent=2))
            print("Temporary Cognito pool, domain, user, client and branding removed.")


if __name__ == "__main__":
    main()
