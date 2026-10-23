"""Build a local, unsigned macOS development app. No installation or model download."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
if sys.platform != "darwin":
    raise SystemExit("This development bundle helper is for macOS. Use cargo run elsewhere.")
subprocess.run(["cargo", "build", "--locked", "-p", "symbio-desktop"], cwd=root, check=True)
bundle = root / "target/debug/Symbio.app/Contents"
(bundle / "MacOS").mkdir(parents=True, exist_ok=True)
(bundle / "Resources").mkdir(exist_ok=True)
shutil.copy2(root / "target/debug/symbio-desktop", bundle / "MacOS/symbio-desktop")
(bundle / "Info.plist").write_bytes(plistlib.dumps({
    "CFBundleName": "Symbio", "CFBundleDisplayName": "Symbio", "CFBundleIdentifier": "dev.symbio.workbench",
    "CFBundleExecutable": "symbio-desktop", "CFBundlePackageType": "APPL",
    "CFBundleVersion": "0.1.0", "CFBundleShortVersionString": "0.1.0",
    "NSHighResolutionCapable": True, "NSPrincipalClass": "NSApplication",
}))
print(bundle.parent)
