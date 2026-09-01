#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time


HOME = Path.home()
PREFIX = HOME / ".local" / "lib" / "skwd-theme-matrix"
SOURCE = PREFIX / "src"
BUILD = PREFIX / "build"
PROVIDERS = ("caelestia", "dms", "noctalia", "end4")
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


class GuestError(RuntimeError):
    pass


def command(argv, **kwargs):
    print("+", " ".join(str(item) for item in argv), flush=True)
    return subprocess.run([str(item) for item in argv], check=True, **kwargs)


def clone_pinned(name, spec):
    target = SOURCE / name
    if not target.exists():
        command(["git", "clone", "--filter=blob:none", "--no-checkout", spec["url"], target])
    command(["git", "-C", target, "fetch", "--depth", "1", "origin", spec["commit"]])
    command(["git", "-C", target, "checkout", "--detach", spec["commit"]])
    command(["git", "-C", target, "submodule", "update", "--init", "--recursive", "--depth", "1"])
    actual = command(["git", "-C", target, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if actual != spec["commit"]:
        raise GuestError(f"{name}: expected {spec['commit']}, got {actual}")
    return target


def provision(args):
    data = json.loads(Path(args.pins).read_text())
    packages = [
        "base-devel", "git", "python", "jq", "go", "cmake", "ninja", "meson", "pkgconf",
        "quickshell", "sway", "hyprland", "grim", "matugen", "dbus", "xorg-xwayland",
        "ttf-material-symbols-variable", "ttf-jetbrains-mono-nerd", "ttf-roboto",
        "qt6-base", "qt6-declarative", "qt6-shadertools", "qt6-quick3d", "qt6-imageformats",
        "qt6-5compat", "qt6-positioning", "kirigami", "syntax-highlighting",
        "libqalculate", "pipewire", "wireplumber", "aubio", "cava", "fftw", "lm_sensors",
        "wayland", "wayland-protocols", "libglvnd", "freetype2", "fontconfig", "cairo", "pango",
        "harfbuzz", "libxkbcommon", "glib2", "libsecret", "libsodium", "sdbus-cpp", "polkit",
        "pam", "curl", "libwebp", "libjxl", "libsndfile", "librsvg", "libxml2", "md4c",
        "tomlplusplus", "libical", "nlohmann-json", "stb", "jemalloc",
    ]
    command(["sudo", "pacman", "-Syu", "--noconfirm", "--needed", *packages])
    command(["sudo", "usermod", "-a", "-G", "seat", os.environ.get("USER", "skwd")])
    SOURCE.mkdir(parents=True, exist_ok=True)
    BUILD.mkdir(parents=True, exist_ok=True)
    providers = {name: clone_pinned(name, spec) for name, spec in data["providers"].items()}

    libcava = clone_pinned("libcava", data["dependencies"]["libcava"])
    libcava_build = BUILD / "libcava"
    command(["cmake", "-S", libcava, "-B", libcava_build, "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release"])
    command(["cmake", "--build", libcava_build, "-j4"])
    include_dir = PREFIX / "include" / "cava"
    library_dir = PREFIX / "lib"
    pkgconfig_dir = library_dir / "pkgconfig"
    include_dir.mkdir(parents=True, exist_ok=True)
    pkgconfig_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(libcava / "cavacore.h", include_dir / "cavacore.h")
    shutil.copy2(libcava_build / "libcavacore.a", library_dir / "libcavacore.a")
    (pkgconfig_dir / "libcava.pc").write_text(
        f"prefix={PREFIX}\n"
        "exec_prefix=${prefix}\n"
        "libdir=${exec_prefix}/lib\n"
        "includedir=${prefix}/include\n\n"
        "Name: libcava\n"
        "Description: pinned Cava core for the Caelestia compatibility guest\n"
        "Version: 0.10.7\n"
        "Libs: -L${libdir} -lcavacore -lfftw3 -lm\n"
        "Cflags: -I${includedir}\n"
    )

    dms = providers["dms"]
    command(["make", "build"], cwd=dms)
    command(["make", "install-bin", "install-shell", f"PREFIX={PREFIX}"], cwd=dms)

    caelestia = providers["caelestia"]
    caelestia_build = BUILD / "caelestia"
    build_env = os.environ.copy()
    build_env["PKG_CONFIG_PATH"] = str(pkgconfig_dir)
    command([
        "cmake", "-S", caelestia, "-B", caelestia_build, "-G", "Ninja",
        "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={PREFIX}",
        "-DVERSION=1.0.0", f"-DGIT_REVISION={data['providers']['caelestia']['commit']}",
        "-DDISTRIBUTOR=skwd-theme-matrix", "-DENABLE_MODULES=extras;plugin;shell;m3shapes",
        f"-DINSTALL_QSCONFDIR={PREFIX}/share/quickshell/caelestia",
    ], env=build_env)
    command(["cmake", "--build", caelestia_build, "-j4"])
    command(["cmake", "--install", caelestia_build])

    noctalia = providers["noctalia"]
    noctalia_build = BUILD / "noctalia"
    command([
        "meson", "setup", noctalia_build, noctalia, "--buildtype=release",
        f"--prefix={PREFIX}", "-Dtests=disabled", "-Djemalloc=auto",
    ])
    command(["meson", "compile", "-C", noctalia_build, "-j4"])
    command(["meson", "install", "-C", noctalia_build])

    marker = PREFIX / "prepared.json"
    marker.write_text(json.dumps({
        "schema": 1,
        "pins": {name: spec["commit"] for name, spec in data["providers"].items()},
        "dependencies": {name: spec["commit"] for name, spec in data["dependencies"].items()},
        "prepared": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }, indent=2) + "\n")
    return 0


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
    if (width, height) != (1280, 720) or screenshot.stat().st_size < 4096:
        raise GuestError(f"{name}: invalid screenshot geometry or payload")
    return {"file": screenshot.name, "width": width, "height": height}


def run_provider(name, contract, value, env, results):
    native = provider_path(name, env)
    log_path = results / f"{name}.log"
    with log_path.open("wb") as log:
        process = subprocess.Popen(
            [str(item) for item in provider_command(name)], env=env,
            stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
        )
        try:
            wait_process(process, 8, name)
            screenshot = capture_provider(name, env, results, log)
        finally:
            terminate(process)
    compositor = validate_compositor(name, env, log_path)
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
        "shell_process": "stable for 8s",
        "compositor": compositor,
        "compositor_contract": "pass",
        "screenshot": screenshot,
        "outbound": f"skwd-to-{name}.json",
        "inbound": f"{name}-to-skwd.json",
        "log": f"{name}.log",
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

    report = {"schema": 2, "providers": {}, "pins": json.loads(Path(args.pins).read_text())["providers"]}
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
                name, contract, values[name], sway_env, results
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
                name, contract, values[name], hyprland_env, results
            )
    finally:
        if hyprland:
            terminate(hyprland)
        if hyprland_log:
            hyprland_log.close()
    report["status"] = "pass"
    (results / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    lines = [
        "# Skwd desktop-theme provider VM matrix", "",
        "| Provider | Contract | Compositor | Shell | Screenshot |",
        "| --- | --- | --- | --- | --- |",
    ]
    for name in PROVIDERS:
        item = report["providers"][name]
        screenshot = item["screenshot"]
        lines.append(
            f"| {name} | PASS | {item['compositor']} PASS | PASS | "
            f"{screenshot['width']}x{screenshot['height']} |"
        )
    lines += [
        "",
        "The contract columns use the current Deck encoder/decoder binary copied into the guest.",
        "Caelestia and end4 run under direct DRM Hyprland; DMS and Noctalia run under headless Sway.",
        "Compositor PASS includes session identity and required protocol-log checks.",
        "Shell PASS means the pinned real process accepted its published native file and stayed alive for the observation window.",
        "Screenshots are captured from the provider's own compositor at the required output geometry.",
        "GPU-native visual acceptance remains a separate workstation test.",
        "",
    ]
    (results / "REPORT.md").write_text("\n".join(lines))
    return 0


def parse_args():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    provision_parser = subparsers.add_parser("provision")
    provision_parser.add_argument("--pins", required=True)
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--pins", required=True)
    run_parser.add_argument("--contract", required=True)
    run_parser.add_argument("--results", required=True)
    return parser.parse_args()


def main():
    args = parse_args()
    try:
        if args.command == "provision":
            return provision(args)
        return run_matrix(args)
    except (GuestError, OSError, ValueError, subprocess.CalledProcessError) as err:
        print(f"theme-provider-matrix guest: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
