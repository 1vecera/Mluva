# /// script
# requires-python = ">=3.13"
# dependencies = ["boto3>=1.40"]
# ///
"""Prepare or apply Mluva's managed-login style to one existing Cognito client."""

import argparse
import hashlib
import json
import xml.etree.ElementTree as ET
from pathlib import Path

import boto3


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--user-pool-id", required=True)
    parser.add_argument("--client-id", required=True)
    parser.add_argument("--region", default="eu-central-1")
    parser.add_argument("--account", default="762052017888")
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Apply after deployment approval; otherwise read and report only.",
    )
    args = parser.parse_args()
    account = boto3.client("sts", region_name=args.region).get_caller_identity()[
        "Account"
    ]
    if account != args.account:
        parser.error("The AWS account differs from --account; nothing changed.")
    client = boto3.client("cognito-idp", region_name=args.region)
    pool = client.describe_user_pool(UserPoolId=args.user_pool_id)["UserPool"]
    if pool["UserPoolTier"] not in {"ESSENTIALS", "PLUS"}:
        parser.error(
            "Managed login needs an existing Essentials or Plus tier. This command never changes pricing tiers."
        )
    client.describe_user_pool_client(
        UserPoolId=args.user_pool_id, ClientId=args.client_id
    )
    root = Path(__file__).resolve().parents[2]
    settings_file = root / "hosted/branding/settings.json"
    assets = []
    digests = {"settings.json": hashlib.sha256(settings_file.read_bytes()).hexdigest()}
    for mode in ["LIGHT", "DARK"]:
        path = root / f"docs/brand/png/mluva-logo-medium-mark-on-{mode.lower()}.png"
        assets.append(
            {
                "Category": "FORM_LOGO",
                "ColorMode": mode,
                "Extension": "PNG",
                "Bytes": path.read_bytes(),
            }
        )
        digests[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    favicon = root / "docs/brand/svg/mluva-mark-flat.svg"
    # Cognito's SVG allowlist excludes HTML accessibility attributes; favicon pixels are unchanged.
    ET.register_namespace("", "http://www.w3.org/2000/svg")
    icon = ET.fromstring(favicon.read_bytes())
    for attribute in ["role", "aria-labelledby"]:
        icon.attrib.pop(attribute, None)
    assets.append(
        {
            "Category": "FAVICON_SVG",
            "ColorMode": "DYNAMIC",
            "Extension": "SVG",
            "Bytes": ET.tostring(icon),
        }
    )
    digests[favicon.name] = hashlib.sha256(favicon.read_bytes()).hexdigest()
    try:
        current = client.describe_managed_login_branding_by_client(
            UserPoolId=args.user_pool_id,
            ClientId=args.client_id,
            ReturnMergedResources=False,
        )["ManagedLoginBranding"]
    except client.exceptions.ResourceNotFoundException:
        current = None
    report = {
        "account": account,
        "region": args.region,
        "userPool": args.user_pool_id,
        "client": args.client_id,
        "tierUnchanged": pool["UserPoolTier"],
        "action": "update" if current else "create",
        "applied": False,
        "sha256": digests,
    }
    if args.apply:
        payload = {
            "UserPoolId": args.user_pool_id,
            "UseCognitoProvidedValues": False,
            "Settings": json.loads(settings_file.read_text()),
            "Assets": assets,
        }
        if current:
            result = client.update_managed_login_branding(
                ManagedLoginBrandingId=current["ManagedLoginBrandingId"], **payload
            )
        else:
            result = client.create_managed_login_branding(
                ClientId=args.client_id, **payload
            )
        report.update(
            applied=True,
            branding=result["ManagedLoginBranding"]["ManagedLoginBrandingId"],
        )
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
