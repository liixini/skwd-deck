#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time


HOME = Path.home()
PREFIX = HOME / ".local" / "lib" / "skwd-theme-matrix"
SOURCE = PREFIX / "src"
PROVIDERS = ("caelestia", "dms", "noctalia", "end4")
PREVIEW_PROVIDERS = ("dms", "noctalia")
PROVIDER_COMPOSITORS = {
    "caelestia": "hyprland",
    "dms": "sway",
    "noctalia": "sway",
    "end4": "hyprland",
}
HYPRLAND_FAILURES = (
    "$HYPRLAND_INSTANCE_SIGNATURE is unset",
    "Cannot connect to hyprland",
    "Unable to connect to hyprland event socket",
    "unable to connect to Hyprland event socket",
    "Hyprland ipc status request failed",
    "does not support hyprland_global_shortcuts_v1",
    "does not support hyprland-toplevel-mapping-v1",
)
CANONICAL_KEYS = {
    "primary", "primaryText", "primaryContainer", "primaryContainerText",
    "secondary", "secondaryText", "secondaryContainer", "secondaryContainerText",
    "tertiary", "tertiaryText", "tertiaryContainer", "tertiaryContainerText",
    "background", "backgroundText", "surface", "surfaceText", "surfaceVariant",
    "surfaceVariantText", "surfaceContainer", "outline", "shadow", "inverseSurface",
    "inverseSurfaceText", "inversePrimary", "error", "errorText", "errorContainer",
    "errorContainerText", "onPrimary",
}
RECORDED_PACKAGES = [
    "quickshell", "hyprland", "sway", "matugen", "grim", "qt6-base", "qt6-declarative",
    "mesa", "python-pillow", "linux",
]
SCREEN = (1280, 720)
PIXEL_TOLERANCE = 12
PIXEL_FLOOR = 100
MIN_SATURATION = 0.25
MIN_VALUE = 96
PROBE_COLOR = (0x22, 0x66, 0xDD)
SHELL_SETTLE_SECONDS = 8


class GuestError(RuntimeError):
    pass


def command(argv, **kwargs):
    print("+", " ".join(str(item) for item in argv), flush=True)
    return subprocess.run([str(item) for item in argv], check=True, **kwargs)


def wait_process(process, seconds, name):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        code = process.poll()
        if code is not None:
            raise GuestError(f"{name} exited during startup with status {code}")
        time.sleep(0.2)


def terminate(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def provider_path(name, env):
    if name == "caelestia":
        return Path(env["XDG_STATE_HOME"]) / "caelestia" / "scheme.json"
    if name == "dms":
        return Path(env["XDG_CACHE_HOME"]) / "DankMaterialShell" / "dms-colors.json"
    if name == "noctalia":
        return Path(env["XDG_CONFIG_HOME"]) / "noctalia" / "palettes" / "skwd-wall.json"
    return Path(env["XDG_STATE_HOME"]) / "quickshell" / "user" / "generated" / "colors.json"


def validate_native(name, value):
    if name == "caelestia":
        colors = value.get("colours")
        if not isinstance(colors, dict) or len(colors) < 49:
            raise GuestError("caelestia: incomplete colours document")
    elif name == "dms":
        colors = value.get("colors")
        if not isinstance(colors, dict) or not {"dark", "light"} <= set(colors):
            raise GuestError("dms: missing dark/light colors")
    elif name == "noctalia":
        if not {"dark", "light"} <= set(value):
            raise GuestError("noctalia: missing dark/light palette")
    elif len(value) < 49:
        raise GuestError("end4: incomplete Material role document")


def mutate_native(name, value):
    changed = json.loads(json.dumps(value))
    if name == "caelestia":
        changed["colours"]["primary"] = "11aa77"
    elif name == "dms":
        changed["colors"]["dark"]["primary"] = "#11aa77"
    elif name == "noctalia":
        changed["dark"]["mPrimary"] = "#11aa77"
    else:
        changed["primary"] = "#11aa77"
    return changed


def parse_hex(value):
    if not isinstance(value, str):
        return None
    digits = value.strip().lstrip("#")
    if len(digits) != 6:
        return None
    try:
        return tuple(int(digits[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        return None


def chromatic(rgb):
    high = max(rgb)
    low = min(rgb)
    return high >= MIN_VALUE and (high - low) / high >= MIN_SATURATION


def published_colors(name, value):
    if name == "caelestia":
        raw = value.get("colours", {}).values()
    elif name == "dms":
        colors = value.get("colors", {})
        raw = [*colors.get("dark", {}).values(), *colors.get("light", {}).values()]
    elif name == "noctalia":
        raw = []
        for mode in ("dark", "light"):
            raw.extend(hex for key, hex in value.get(mode, {}).items() if key.startswith("m"))
    else:
        raw = value.values()
    colors = set()
    for item in raw:
        rgb = parse_hex(item)
        if rgb and chromatic(rgb):
            colors.add(rgb)
    return colors


def near(rgb, candidates):
    return any(
        abs(rgb[0] - c[0]) <= PIXEL_TOLERANCE
        and abs(rgb[1] - c[1]) <= PIXEL_TOLERANCE
        and abs(rgb[2] - c[2]) <= PIXEL_TOLERANCE
        for c in candidates
    )


def coverage(histogram, candidates):
    matched = 0
    per_color = {}
    for count, rgb in histogram:
        if near(rgb, candidates):
            matched += count
            for c in candidates:
                if (
                    abs(rgb[0] - c[0]) <= PIXEL_TOLERANCE
                    and abs(rgb[1] - c[1]) <= PIXEL_TOLERANCE
                    and abs(rgb[2] - c[2]) <= PIXEL_TOLERANCE
                ):
                    key = "#%02x%02x%02x" % c
                    per_color[key] = per_color.get(key, 0) + count
                    break
    return matched, per_color


def screenshot_histogram(path):
    from PIL import Image

    with Image.open(path) as image:
        rgb = image.convert("RGB")
        if rgb.size != SCREEN:
            raise GuestError(f"{path.name}: unexpected geometry {rgb.size}")
        histogram = rgb.getcolors(maxcolors=rgb.size[0] * rgb.size[1])
    if not histogram:
        raise GuestError(f"{path.name}: could not build a colour histogram")
    return histogram


def visual_check(name, screenshot, value):
    candidates = sorted(published_colors(name, value))
    if not candidates:
        raise GuestError(f"{name}: published palette has no chromatic roles to look for")
    histogram = screenshot_histogram(screenshot)
    matched, per_color = coverage(histogram, candidates)
    decoys = [(255 - r, 255 - g, 255 - b) for r, g, b in candidates]
    decoy_matched, _ = coverage(histogram, decoys)
    dominant = [
        {"color": "#%02x%02x%02x" % rgb, "pixels": count}
        for count, rgb in sorted(histogram, reverse=True)[:8]
    ]
    result = {
        "candidates": ["#%02x%02x%02x" % c for c in candidates],
        "tolerance": PIXEL_TOLERANCE,
        "floor": PIXEL_FLOOR,
        "matched": matched,
        "decoyMatched": decoy_matched,
        "perColor": dict(sorted(per_color.items(), key=lambda item: -item[1])),
        "dominant": dominant,
    }
    if matched < PIXEL_FLOOR:
        raise GuestError(
            f"{name}: only {matched} screenshot pixels carry the published palette (floor {PIXEL_FLOOR}); "
            f"dominant colours {[item['color'] for item in dominant]}"
        )
    if matched <= 2 * decoy_matched:
        raise GuestError(
            f"{name}: palette match {matched} is not distinguishable from the inverted decoy {decoy_matched}"
        )
    return result


def write_probe_image(path):
    from PIL import Image

    Image.new("RGB", (64, 64), PROBE_COLOR).save(path)
    return path


def write_skwd_config(env, name, wallpaper_dir):
    config_dir = Path(env["XDG_CONFIG_HOME"]) / "skwd-wall-v2"
    config_dir.mkdir(parents=True, exist_ok=True)
    document = {
        "paths": {
            "wallpaper": str(wallpaper_dir),
            "noctaliaBin": str(PREFIX / "bin" / "noctalia"),
        },
        "theme": {"policy": "wallpaper", "authority": name, "mode": "dark", "scheme": "tonal-spot"},
        "noctalia": {"hoverPreview": True, "themeMode": "follow"},
        "dms": {"hoverPreview": True},
    }
    (config_dir / "config.json").write_text(json.dumps(document, indent=2) + "\n")


def preview_cycle(name, contract, env, results, probe):
    output = results / f"{name}-preview.json"
    write_skwd_config(env, name, results)
    log_path = results / f"{name}-preview.log"
    with log_path.open("wb") as log:
        result = subprocess.run(
            [str(contract), "preview-cycle", name, str(probe), str(output)],
            env=env, stdout=log, stderr=subprocess.STDOUT,
        )
    if result.returncode != 0 or not output.is_file():
        detail = log_path.read_text(errors="replace").strip()
        raise GuestError(f"{name}: hover preview cycle failed ({result.returncode}): {detail}")
    report = json.loads(output.read_text())
    before, after, restored = report["before"], report["after"], report["restored"]
    if report.get("previewError"):
        raise GuestError(f"{name}: hover preview reported {report['previewError']}")
    if name == "dms":
        if after["native"] == before["native"]:
            raise GuestError("dms: hover preview left dms-colors.json unchanged")
        if restored["native"] != before["native"]:
            raise GuestError("dms: hover preview end did not restore dms-colors.json")
    else:
        if not after.get("hoverPalette"):
            raise GuestError("noctalia: hover preview did not write the skwd-hover palette")
        if after.get("scheme") != "custom skwd-hover":
            raise GuestError(f"noctalia: shell reports {after.get('scheme')!r} during the hover preview")
        if restored.get("scheme") != before.get("scheme"):
            raise GuestError(
                f"noctalia: hover preview end restored {restored.get('scheme')!r}, expected {before.get('scheme')!r}"
            )
        if restored.get("hoverPalette"):
            raise GuestError("noctalia: hover palette file survived the preview end")
        if restored["native"] != before["native"]:
            raise GuestError("noctalia: hover preview touched the applied palette")
    return {
        "status": "pass",
        "sink": report["sink"],
        "before": before.get("scheme"),
        "during": after.get("scheme"),
        "report": output.name,
        "log": log_path.name,
    }


def prime_shell(name, env):
    if name == "dms":
        config_dir = Path(env["XDG_CONFIG_HOME"]) / "DankMaterialShell"
        config_dir.mkdir(parents=True, exist_ok=True)
        (config_dir / "settings.json").write_text(json.dumps({
            "currentThemeName": "dynamic",
            "matugenScheme": "scheme-tonal-spot",
        }, indent=2) + "\n")
        (config_dir / ".firstlaunch").touch()
    elif name == "noctalia":
        state_dir = Path(env["XDG_STATE_HOME"]) / "noctalia"
        state_dir.mkdir(parents=True, exist_ok=True)
        (state_dir / "settings.toml").write_text(
            "[theme]\n"
            "source = \"custom\"\n"
            "custom_palette = \"skwd-wall\"\n"
            "\n"
            "[shell]\n"
            "setup_wizard_enabled = false\n"
        )
        (state_dir / ".setup-complete").touch()


def activate_live(name, contract, env, results):
    log_path = results / f"{name}-activate.log"
    with log_path.open("wb") as log:
        result = subprocess.run(
            [str(contract), "publish", "#42ff77"], env=env, stdout=log, stderr=subprocess.STDOUT,
        )
    if result.returncode != 0:
        raise GuestError(f"{name}: republishing to the live shell failed ({result.returncode})")
    if name != "noctalia":
        return {"status": "pass", "log": log_path.name}
    deadline = time.monotonic() + 10
    scheme = ""
    while time.monotonic() < deadline:
        probe = subprocess.run(
            [str(PREFIX / "bin" / "noctalia"), "msg", "color-scheme-get"], env=env,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=5,
        )
        scheme = probe.stdout.strip()
        if probe.returncode == 0 and scheme == "custom skwd-wall":
            return {"status": "pass", "scheme": scheme, "log": log_path.name}
        time.sleep(0.5)
    raise GuestError(f"noctalia: shell reports {scheme!r} after Deck activated custom skwd-wall")


def provider_command(name):
    if name == "caelestia":
        return ["qs", "-p", PREFIX / "share" / "quickshell" / "caelestia"]
    if name == "dms":
        return [PREFIX / "bin" / "dms", "run"]
    if name == "noctalia":
        return [PREFIX / "bin" / "noctalia"]
    return ["qs", "-p", SOURCE / "end4" / "dots" / ".config" / "quickshell" / "ii"]


def start_sway(env, results):
    config = results / "sway.conf"
    config.write_text(
        "output HEADLESS-1 mode 1280x720\n"
        "seat seat0 hide_cursor 1000\n"
        "default_border none\n"
        "exec true\n"
    )
    log = (results / "sway.log").open("wb")
    sway = subprocess.Popen(
        ["sway", "-c", config], env=env, stdout=log, stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    wait_process(sway, 2, "sway")
    sockets = sorted(Path(env["XDG_RUNTIME_DIR"]).glob("sway-ipc.*.sock"))
    if not sockets:
        terminate(sway)
        raise GuestError("sway did not publish an IPC socket")
    display = sorted(Path(env["XDG_RUNTIME_DIR"]).glob("wayland-*"))
    display = [path for path in display if not path.name.endswith(".lock")]
    if not display:
        terminate(sway)
        raise GuestError("sway did not publish a Wayland display")
    env["SWAYSOCK"] = str(sockets[-1])
    env["WAYLAND_DISPLAY"] = display[-1].name
    return sway, log


def start_hyprland(base_env, results):
    config = results / "hyprland.conf"
    config.write_text(
        "monitor = , 1280x720@60, 0x0, 1\n"
        "animations {\n"
        "    enabled = false\n"
        "}\n"
        "decoration {\n"
        "    blur {\n"
        "        enabled = false\n"
        "    }\n"
        "    shadow {\n"
        "        enabled = false\n"
        "    }\n"
        "}\n"
        "misc {\n"
        "    disable_hyprland_logo = true\n"
        "    disable_splash_rendering = true\n"
        "    force_default_wallpaper = 0\n"
        "}\n"
    )
    log = (results / "hyprland.log").open("wb")
    launch_env = base_env.copy()
    for key in (
        "HYPRLAND_INSTANCE_SIGNATURE", "SWAYSOCK", "WAYLAND_DISPLAY",
        "WLR_BACKENDS", "WLR_LIBINPUT_NO_DEVICES", "WLR_RENDERER",
        "WLR_RENDERER_ALLOW_SOFTWARE",
    ):
        launch_env.pop(key, None)
    launch_env["HYPRLAND_NO_SD_VARS"] = "1"
    launch_env["LIBSEAT_BACKEND"] = "seatd"
    process = subprocess.Popen(
        ["start-hyprland", "--", "--config", str(config)], env=launch_env,
        stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
    )
    deadline = time.monotonic() + 30
    runtime = Path(launch_env["XDG_RUNTIME_DIR"])
    session_env = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            break
        for lock in runtime.glob("hypr/*/hyprland.lock"):
            try:
                lines = lock.read_text().splitlines()
                if len(lines) < 2 or os.getpgid(int(lines[0])) != process.pid:
                    continue
            except (OSError, ValueError):
                continue
            session_env = launch_env.copy()
            session_env.pop("SWAYSOCK", None)
            session_env["HYPRLAND_INSTANCE_SIGNATURE"] = lock.parent.name
            session_env["WAYLAND_DISPLAY"] = lines[1]
            session_env["XDG_CURRENT_DESKTOP"] = "Hyprland"
            session_env["XDG_SESSION_DESKTOP"] = "Hyprland"
            break
        if session_env:
            result = subprocess.run(
                ["hyprctl", "-j", "monitors"], env=session_env,
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
            )
            if result.returncode == 0:
                try:
                    monitors = json.loads(result.stdout)
                except json.JSONDecodeError:
                    monitors = []
                if monitors:
                    (results / "hyprland-monitors.json").write_text(
                        json.dumps(monitors, indent=2) + "\n"
                    )
                    return process, log, session_env
        time.sleep(0.2)
    terminate(process)
    log.close()
    raise GuestError("Hyprland did not publish a usable direct compositor session")


def validate_compositor(name, env, log_path):
    expected = PROVIDER_COMPOSITORS[name]
    text = log_path.read_text(errors="replace")
    if expected == "hyprland":
        if not env.get("HYPRLAND_INSTANCE_SIGNATURE"):
            raise GuestError(f"{name}: missing Hyprland instance identity")
        failures = [pattern for pattern in HYPRLAND_FAILURES if pattern in text]
        if failures:
            raise GuestError(f"{name}: Hyprland integration failed: {failures[0]}")
    elif not env.get("SWAYSOCK"):
        raise GuestError(f"{name}: missing Sway IPC identity")
    return expected


def capture_provider(name, env, results, log):
    screenshot = results / f"{name}.png"
    result = subprocess.run(
        ["grim", screenshot], env=env, stdout=log, stderr=subprocess.STDOUT,
    )
    if result.returncode != 0 or not screenshot.is_file():
        raise GuestError(f"{name}: compositor screenshot failed")
    header = screenshot.read_bytes()[:24]
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
        raise GuestError(f"{name}: screenshot is not a PNG")
    width = int.from_bytes(header[16:20], "big")
    height = int.from_bytes(header[20:24], "big")
    if (width, height) != SCREEN or screenshot.stat().st_size < 4096:
        raise GuestError(f"{name}: invalid screenshot geometry or payload")
    return {"file": screenshot.name, "width": width, "height": height}


def run_provider(name, contract, value, env, results, probe):
    native = provider_path(name, env)
    log_path = results / f"{name}.log"
    preview = None
    prime_shell(name, env)
    with log_path.open("wb") as log:
        process = subprocess.Popen(
            [str(item) for item in provider_command(name)], env=env,
            stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
        )
        try:
            wait_process(process, SHELL_SETTLE_SECONDS, name)
            activation = activate_live(name, contract, env, results)
            wait_process(process, 2, name)
            screenshot = capture_provider(name, env, results, log)
            if name in PREVIEW_PROVIDERS:
                preview = preview_cycle(name, contract, env, results, probe)
                if process.poll() is not None:
                    raise GuestError(f"{name}: shell exited during the hover preview cycle")
        finally:
            terminate(process)
    compositor = validate_compositor(name, env, log_path)
    visual = visual_check(name, results / screenshot["file"], value)
    reverse = mutate_native(name, value)
    native.write_text(json.dumps(reverse, indent=2) + "\n")
    normalized = results / f"{name}-to-skwd.json"
    command([contract, "normalize", name, native, normalized], env=env)
    canonical = json.loads(normalized.read_text())
    if set(canonical) != CANONICAL_KEYS:
        raise GuestError(f"{name}: normalized key set differs from canonical 29-role contract")
    if canonical["primary"] != "#11aa77":
        raise GuestError(f"{name}: reverse primary sentinel was lost")
    return {
        "status": "pass",
        "shell_process": f"stable for {SHELL_SETTLE_SECONDS}s",
        "compositor": compositor,
        "compositor_contract": "pass",
        "screenshot": screenshot,
        "activation": activation,
        "visual": visual,
        "preview": preview,
        "outbound": f"skwd-to-{name}.json",
        "inbound": f"{name}-to-skwd.json",
        "log": f"{name}.log",
    }


def provenance():
    packages = {}
    result = subprocess.run(
        ["pacman", "-Q", *RECORDED_PACKAGES],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
    )
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) == 2:
            packages[parts[0]] = parts[1]
    prepared = {}
    marker = PREFIX / "prepared.json"
    if marker.is_file():
        try:
            prepared = json.loads(marker.read_text())
        except json.JSONDecodeError:
            prepared = {"error": "unreadable prepared.json"}
    return {
        "kernel": platform.release(),
        "packages": packages,
        "prepared": prepared,
        "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }


def run_matrix(args):
    if not (PREFIX / "prepared.json").is_file():
        raise GuestError("guest image is not prepared")
    contract = Path(args.contract)
    if contract.is_symlink() or not os.access(contract, os.X_OK):
        raise GuestError("contract driver is missing or not executable")
    results = Path(args.results)
    results.mkdir(parents=True, exist_ok=False)
    root = HOME / ".local" / "state" / f"skwd-theme-run-{int(time.time())}"
    env = os.environ.copy()
    env.update({
        "PATH": f"{PREFIX}/bin:{env['PATH']}",
        "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_STATE_HOME": str(root / "state"),
        "XDG_DATA_HOME": str(root / "data"),
        "SKWD_WALL_V2_CACHE": str(root / "cache" / "skwd-wall"),
        "LIBGL_ALWAYS_SOFTWARE": "1",
        "QT_QUICK_BACKEND": "software",
        "QML2_IMPORT_PATH": str(PREFIX / "lib" / "qt6" / "qml"),
        "QML_IMPORT_PATH": str(PREFIX / "lib" / "qt6" / "qml"),
        "LD_LIBRARY_PATH": f"{PREFIX}/lib:{PREFIX}/lib/caelestia",
    })
    for key in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "SKWD_WALL_V2_CACHE"]:
        Path(env[key]).mkdir(parents=True, exist_ok=True)
    # Hyprland's signature is long and AF_UNIX paths are limited to 108 bytes.
    # Keep this root deliberately terse so Quickshell can reach both IPC sockets.
    runtime_root = Path("/tmp") / f"stm-{os.getpid()}"
    runtime_root.mkdir(mode=0o700)
    sway_env = env.copy()
    sway_env.update({
        "XDG_RUNTIME_DIR": str(runtime_root / "sway"),
        "WLR_BACKENDS": "headless",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "WLR_RENDERER": "pixman",
    })
    hyprland_env = env.copy()
    hyprland_env["XDG_RUNTIME_DIR"] = str(runtime_root / "hyprland")
    for compositor_env in (sway_env, hyprland_env):
        Path(compositor_env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
    caelestia = provider_path("caelestia", env)
    caelestia.parent.mkdir(parents=True, exist_ok=True)
    caelestia.write_text(json.dumps({
        "name": "native", "flavour": "default", "mode": "dark",
        "variant": "tonalspot", "colours": {},
    }) + "\n")
    probe = write_probe_image(results / "probe.png")

    pins = json.loads(Path(args.pins).read_text())
    report = {
        "schema": 3,
        "label": args.label,
        "providers": {},
        "pins": pins["providers"],
        "provenance": provenance(),
    }
    values = {}
    command(["sudo", "systemctl", "start", "seatd.service"])
    command([contract, "publish", "#42ff77"], env=env, capture_output=True, text=True)
    for name in PROVIDERS:
        native = provider_path(name, env)
        if not native.is_file():
            raise GuestError(f"{name}: Deck did not publish {native}")
        value = json.loads(native.read_text())
        validate_native(name, value)
        values[name] = value
        (results / f"skwd-to-{name}.json").write_text(json.dumps(value, indent=2) + "\n")

    sway = None
    sway_log = None
    try:
        sway, sway_log = start_sway(sway_env, results)
        for name in ("dms", "noctalia"):
            report["providers"][name] = run_provider(
                name, contract, values[name], sway_env, results, probe
            )
    finally:
        if sway:
            terminate(sway)
        if sway_log:
            sway_log.close()

    hyprland = None
    hyprland_log = None
    try:
        hyprland, hyprland_log, hyprland_env = start_hyprland(hyprland_env, results)
        for name in ("caelestia", "end4"):
            report["providers"][name] = run_provider(
                name, contract, values[name], hyprland_env, results, probe
            )
    finally:
        if hyprland:
            terminate(hyprland)
        if hyprland_log:
            hyprland_log.close()
    report["status"] = "pass"
    report["provenance"]["finished"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    (results / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    lines = [
        "# Skwd desktop-theme provider VM matrix", "",
        f"Label: {args.label}", "",
        "| Provider | Contract | Compositor | Shell | Palette pixels | Hover preview | Screenshot |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for name in PROVIDERS:
        item = report["providers"][name]
        screenshot = item["screenshot"]
        preview = item["preview"]
        preview_cell = "n/a" if preview is None else f"PASS ({preview['sink']})"
        lines.append(
            f"| {name} | PASS | {item['compositor']} PASS | PASS | "
            f"{item['visual']['matched']} (decoy {item['visual']['decoyMatched']}) | {preview_cell} | "
            f"{screenshot['width']}x{screenshot['height']} |"
        )
    packages = report["provenance"]["packages"]
    lines += [
        "",
        "Guest packages: " + ", ".join(f"{name} {version}" for name, version in sorted(packages.items())),
        "",
        "The contract columns use the current Deck encoder/decoder binary copied into the guest.",
        "Caelestia and end4 run under direct DRM Hyprland; DMS and Noctalia run under headless Sway.",
        "Compositor PASS includes session identity and required protocol-log checks.",
        "Shell PASS means the pinned real process accepted its published native file and stayed alive for the observation window.",
        "Palette pixels counts screenshot pixels within the tolerance of a chromatic published role; the decoy count uses the inverted palette and must stay well below it.",
        "Hover preview runs Deck's real preview sink against the live shell and requires the shell state to change and then restore.",
        "Screenshots are captured from the provider's own compositor at the required output geometry.",
        "GPU-native visual acceptance remains a separate workstation test.",
        "",
    ]
    (results / "REPORT.md").write_text("\n".join(lines))
    return 0


def parse_args():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--pins", required=True)
    run_parser.add_argument("--contract", required=True)
    run_parser.add_argument("--results", required=True)
    run_parser.add_argument("--label", default="pinned")
    return parser.parse_args()


def main():
    args = parse_args()
    try:
        return run_matrix(args)
    except (GuestError, OSError, ValueError, subprocess.CalledProcessError) as err:
        print(f"theme-provider-matrix guest: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
