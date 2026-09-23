"""Install the widget bundled with this Mluva release without a separate repository."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

PLUGIN_ID = "mluva.dictation"
RECEIPT = ".mluva-bundle.json"
REPOSITORY = "https://github.com/1vecera/Mluva"
LEGACY_ORIGINS = {
    "https://github.com/1vecera/omarchy-mluva.git",
    "https://github.com/1vecera/omarchy-mluva",
    "git@github.com:1vecera/omarchy-mluva.git",
}


def file_hashes(directory: Path) -> dict[str, str]:
    """Record all installed content, rejecting links and excluding only the receipt."""
    hashes = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Preserved symbolic link: {path}")
        if path.is_file() and path != directory / RECEIPT:
            hashes[path.relative_to(directory).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def existing_plugin(plugins: Path) -> Path | None:
    """Find the widget by manifest ID, including older manually renamed directories."""
    candidates = []
    for directory in sorted(plugins.iterdir()) if plugins.exists() else []:
        if directory.name.startswith("."):
            continue
        manifest = directory / "manifest.json"
        try:
            is_mluva = json.loads(manifest.read_text()).get("id") == PLUGIN_ID
        except (OSError, ValueError):
            is_mluva = False
        if directory.name == PLUGIN_ID or is_mluva:
            candidates.append(directory)
    if len(candidates) > 1:
        raise ValueError("Multiple Mluva widgets found. Keep one installation before updating.")
    return candidates[0] if candidates else None


def check_existing(directory: Path | None) -> None:
    """Protect local edits while accepting an unchanged bundle or legacy Git checkout."""
    if directory is None:
        return
    if directory.is_symlink():
        raise ValueError(f"Preserved linked plugin: {directory}")
    receipt_path = directory / RECEIPT
    if receipt_path.is_file():
        receipt = json.loads(receipt_path.read_text())
        if receipt.get("repository") == REPOSITORY and receipt.get("sha256") == file_hashes(directory):
            return
    elif (directory / ".git").is_dir():

        def git(*arguments: str) -> str:
            """Inspect only local history; retired upstreams need not be reachable."""
            return subprocess.check_output(["git", "-C", str(directory), *arguments], text=True).strip()

        origin = git("remote", "get-url", "origin")
        if (
            origin in LEGACY_ORIGINS
            and not git("status", "--porcelain", "--untracked-files=all")
            and not git("rev-list", "HEAD", "--not", "--remotes=origin")
        ):
            return
    raise ValueError(
        f"Preserved unmanaged or locally edited widget: {directory}. "
        "Back it up outside the plugins directory before retrying, or use --app-only."
    )


def install_widget(source: Path, home: Path, *, check_only: bool = False) -> None:
    """Replace one owned widget with rollback, retaining the old copy outside discovery."""
    plugins = home / ".config/omarchy/plugins"
    target = plugins / PLUGIN_ID
    previous = existing_plugin(plugins)
    check_existing(previous)
    manifest = json.loads((source / "manifest.json").read_text())
    if manifest.get("id") != PLUGIN_ID:
        raise ValueError("The bundled widget has an unexpected identity.")
    hashes = file_hashes(source)
    subprocess.run(["omarchy", "plugin", "validate", str(source)], check=True)
    if check_only:
        return

    # A new entry-point URL makes Quickshell load updated QML without restarting the bar.
    content_id = hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()[:16]
    bundle_name = f"bundle-{content_id}"
    plugins.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".mluva-stage-", dir=plugins.parent) as temporary:
        stage = Path(temporary) / PLUGIN_ID
        bundle = stage / bundle_name
        shutil.copytree(source, bundle)
        (bundle / "manifest.json").unlink()
        manifest["entryPoints"] = {key: f"{bundle_name}/{path}" for key, path in manifest["entryPoints"].items()}
        (stage / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        (stage / RECEIPT).write_text(
            json.dumps({"repository": REPOSITORY, "sha256": file_hashes(stage)}, indent=2) + "\n"
        )
        subprocess.run(["omarchy", "plugin", "validate", str(stage)], check=True)
        plugins.mkdir(exist_ok=True)
        backup = None
        if previous is not None:
            backup_root = plugins.parent / "plugin-backups"
            backup_root.mkdir(exist_ok=True)
            backup = Path(tempfile.mkdtemp(prefix=f"{PLUGIN_ID}-", dir=backup_root)) / previous.name
            previous.rename(backup)
        try:
            stage.rename(target)
            subprocess.run(["omarchy-shell", "shell", "rescanPlugins"], check=True)
            # Omarchy scans manifests asynchronously after acknowledging the IPC call.
            for _attempt in range(40):
                discovered = subprocess.check_output(["omarchy", "plugin", "list", "--json"], text=True)
                if any(plugin.get("id") == PLUGIN_ID for plugin in json.loads(discovered)):
                    break
                time.sleep(0.05)
            else:
                raise ValueError("Omarchy did not discover the installed widget.")
            subprocess.run(["omarchy", "plugin", "enable", PLUGIN_ID], check=True)
        except (OSError, ValueError, subprocess.CalledProcessError):
            if target.exists():
                shutil.rmtree(target)
            if backup is not None:
                backup.rename(previous)
            subprocess.run(["omarchy-shell", "shell", "rescanPlugins"], check=False)
            raise
    print(f"Installed bundled widget {manifest['version']} at {target}")
    if backup is not None:
        print(f"Previous widget preserved at {backup}")


def main() -> None:
    """Install from a checkout/archive, or inspect ownership before native app installation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Validate without changing the installation")
    args = parser.parse_args()
    try:
        install_widget(Path(__file__).parent / "quickshell" / PLUGIN_ID, Path.home(), check_only=args.check)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Mluva widget setup: {error}\n")


if __name__ == "__main__":
    main()
