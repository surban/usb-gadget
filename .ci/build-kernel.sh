#!/usr/bin/env bash
#
# Build a kernel + modules tarball for CI testing with virtme-ng.
#
# This script:
#   1. Downloads a pinned Linux kernel source tarball.
#   2. Builds it with virtme-ng using the USB gadget config snippet.
#   3. Installs modules to a staging directory.
#   4. Packages bzImage + modules into a compressed tarball.
#
# The resulting tarball is uploaded as a GitHub Release asset and
# downloaded by the CI workflow to run tests inside a VM — no VM
# image, no SSH, no cloud-init needed.
#
# Requirements:
#   - virtme-ng (pip install virtme-ng)
#   - busybox
#   - Standard kernel build deps (gcc, make, flex, bison, libelf, etc.)
#   - zstd
#
# Usage:
#   .ci/build-kernel.sh [--output <path>] [--kernel-version <version>]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ─── Defaults ─────────────────────────────────────────────────────────

KERNEL_VERSION="6.15.3"
OUTPUT=""
WORK_DIR="/tmp/ci-kernel-build"

# ─── Parse arguments ──────────────────────────────────────────────────

while [ $# -gt 0 ]; do
    case "$1" in
        --output|-o)
            OUTPUT="$2"; shift 2 ;;
        --kernel-version|-k)
            KERNEL_VERSION="$2"; shift 2 ;;
        --work-dir|-w)
            WORK_DIR="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: $0 [--output <path>] [--kernel-version <version>] [--work-dir <dir>]"
            echo ""
            echo "Options:"
            echo "  --output, -o          Output tarball path (default: .ci/kernel-<version>.tar.zst)"
            echo "  --kernel-version, -k  Linux kernel version to build (default: $KERNEL_VERSION)"
            echo "  --work-dir, -w        Working directory for kernel source (kept after build)"
            echo "  --help, -h            Show this help"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

KERNEL_MAJOR="${KERNEL_VERSION%%.*}"
: "${OUTPUT:="${SCRIPT_DIR}/kernel-${KERNEL_VERSION}.tar.zst"}"
OUTPUT="$(realpath "$OUTPUT")"

mkdir -p "$WORK_DIR"

# ─── Preflight checks ────────────────────────────────────────────────

for cmd in vng zstd make gcc flex bison; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "ERROR: required command '$cmd' not found." >&2
        exit 1
    fi
done

echo "Kernel version:  $KERNEL_VERSION"
echo "Config snippet:  $SCRIPT_DIR/usb-gadget.config"
echo "Working dir:     $WORK_DIR"
echo "Output:          $OUTPUT"
echo ""

# ─── Download kernel source ──────────────────────────────────────────

KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v${KERNEL_MAJOR}.x/linux-${KERNEL_VERSION}.tar.xz"
KERNEL_TAR="$WORK_DIR/linux-${KERNEL_VERSION}.tar.xz"
KERNEL_SRC="$WORK_DIR/linux-${KERNEL_VERSION}"

if [ -d "$KERNEL_SRC" ]; then
    echo "Kernel source already present at $KERNEL_SRC, skipping download."
else
    if [ ! -f "$KERNEL_TAR" ]; then
        echo "Downloading linux-${KERNEL_VERSION}..."
        wget -q --show-progress -O "$KERNEL_TAR" "$KERNEL_URL"
    fi
    echo "Extracting..."
    tar xf "$KERNEL_TAR" -C "$WORK_DIR"
fi

echo ""

# ─── Build kernel ────────────────────────────────────────────────────

echo "Building kernel (this takes a few minutes)..."
cd "$KERNEL_SRC"

time vng --build --config "$SCRIPT_DIR/usb-gadget.config" --force

echo ""

# ─── Verify the built config has what we need ─────────────────────────

echo "Verifying kernel config..."
MISSING=false
for sym in USB_GADGET USB_DUMMY_HCD USB_CONFIGFS USB_LIBCOMPOSITE \
           USB_F_FS USB_F_ACM USB_F_SERIAL USB_F_ECM USB_F_EEM \
           USB_F_NCM USB_F_RNDIS USB_F_SUBSET USB_F_HID \
           USB_F_MASS_STORAGE USB_F_PRINTER USB_F_MIDI USB_F_UAC2 \
           USB_F_UVC CONFIGFS_FS; do
    if ! grep -q "CONFIG_${sym}=[ym]" .config; then
        echo "  MISSING: CONFIG_${sym}"
        MISSING=true
    fi
done

if [ "$MISSING" = true ]; then
    echo ""
    echo "ERROR: some required config options are not set." >&2
    echo "Check .ci/usb-gadget.config and kernel dependency changes." >&2
    exit 1
fi
echo "  All required options present."
echo ""

# ─── Install modules to staging directory ─────────────────────────────

STAGING="$WORK_DIR/staging"

echo "Installing modules to staging directory..."

rm -rf "$STAGING"
mkdir -p "$STAGING/lib/modules"

make modules_install INSTALL_MOD_PATH="$STAGING" > /dev/null

# Detect the actual kernel release from the installed modules directory.
# We cannot rely on `make kernelrelease` because vng passes LOCALVERSION
# at build time (e.g. -virtme) which is not reflected in the .config.
KVER_FULL="$(ls "$STAGING/lib/modules/" | head -1)"
if [ -z "$KVER_FULL" ]; then
    echo "ERROR: modules_install produced no output in $STAGING/lib/modules/" >&2
    exit 1
fi

echo "Kernel release:  $KVER_FULL"

# Remove the source/build symlinks (they point to the build machine).
rm -f "$STAGING/lib/modules/$KVER_FULL/source"
rm -f "$STAGING/lib/modules/$KVER_FULL/build"

# Copy the kernel image.
mkdir -p "$STAGING/boot"
cp arch/x86/boot/bzImage "$STAGING/boot/bzImage"

# Write a metadata file so CI knows what kernel version this is.
cat > "$STAGING/boot/kernel-info.txt" <<EOF
version=$KVER_FULL
base_version=$KERNEL_VERSION
build_date=$(date -u +%Y-%m-%dT%H:%M:%SZ)
config_snippet=.ci/usb-gadget.config
EOF

echo ""

# ─── Verify modules exist ────────────────────────────────────────────

echo "Verifying built modules..."
ALL_OK=true
for m in dummy_hcd libcomposite usb_f_fs usb_f_acm usb_f_serial \
         usb_f_ecm usb_f_eem usb_f_ncm usb_f_rndis usb_f_ecm_subset \
         usb_f_hid usb_f_mass_storage usb_f_printer usb_f_midi \
         usb_f_uac2 usb_f_uvc; do
    modpath=$(find "$STAGING/lib/modules/$KVER_FULL" -name "${m}.ko*" 2>/dev/null | head -1)
    if [ -z "$modpath" ]; then
        echo "  MISSING: $m"
        ALL_OK=false
    else
        echo "  OK: $m"
    fi
done

if [ "$ALL_OK" != true ]; then
    echo ""
    echo "ERROR: some modules were not built." >&2
    exit 1
fi
echo ""

# ─── Package tarball ──────────────────────────────────────────────────

echo "Creating tarball..."
tar cf - -C "$STAGING" boot lib | zstd -12 -T0 -o "$OUTPUT"

TARBALL_SIZE=$(du -h "$OUTPUT" | cut -f1)
MODULES_COUNT=$(find "$STAGING/lib/modules/$KVER_FULL" -name '*.ko*' | wc -l)

echo ""
echo "════════════════════════════════════════════════════════"
echo "  Kernel build complete"
echo "════════════════════════════════════════════════════════"
echo "  Kernel:     $KVER_FULL"
echo "  Modules:    $MODULES_COUNT total"
echo "  Tarball:    $OUTPUT"
echo "  Size:       $TARBALL_SIZE"
echo "════════════════════════════════════════════════════════"

