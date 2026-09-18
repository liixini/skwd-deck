#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


HOME = Path.home()
PREFIX = HOME / ".local" / "lib" / "skwd-theme-matrix"
SOURCE = PREFIX / "src"
BUILD = PREFIX / "build"
PACKAGES = [
    "base-devel", "git", "python", "python-pillow", "jq", "go", "cmake", "ninja", "meson", "pkgconf",
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
RECORDED_PACKAGES = [
    "quickshell", "hyprland", "sway", "matugen", "grim", "qt6-base", "qt6-declarative",
    "mesa", "python-pillow", "linux",
]


class ProvisionError(RuntimeError):
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
        raise ProvisionError(f"{name}: expected {spec['commit']}, got {actual}")
    return target


def installed_packages(names):
    result = subprocess.run(
        ["pacman", "-Q", *names], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
    )
    versions = {}
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) == 2:
            versions[parts[0]] = parts[1]
    return versions


def provision(pins_path):
    data = json.loads(Path(pins_path).read_text())
    command(["sudo", "pacman", "-Syu", "--noconfirm", "--needed", *PACKAGES])
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
        "schema": 2,
        "pins": {name: spec["commit"] for name, spec in data["providers"].items()},
        "dependencies": {name: spec["commit"] for name, spec in data["dependencies"].items()},
        "packages": installed_packages(RECORDED_PACKAGES),
        "prepared": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }, indent=2) + "\n")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--pins", required=True)
    args = parser.parse_args()
    try:
        return provision(args.pins)
    except (ProvisionError, OSError, ValueError, subprocess.CalledProcessError) as err:
        print(f"theme-provider-matrix provision: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
