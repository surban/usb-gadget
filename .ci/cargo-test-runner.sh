#!/usr/bin/env bash
#
# Cargo test runner that executes test binaries inside a virtme-ng VM
# with dummy_hcd USB gadget support.
#
# Cargo invokes this as:
#   .ci/cargo-test-runner.sh <test-binary> [args...]
#
# Setup (one of):
#   1. Set via environment:
#        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=".ci/cargo-test-runner.sh"
#
#   2. Set in .cargo/config.toml:
#        [target.x86_64-unknown-linux-gnu]
#        runner = ".ci/cargo-test-runner.sh"
#
# The kernel tarball (built by .ci/build-kernel.sh) is located automatically
# from .ci/kernel-*.tar.zst, or set KERNEL_TARBALL=/path/to/tarball.
#
# On first invocation the tarball is extracted to a staging directory that
# is reused for subsequent invocations (within the same tmpdir lifetime).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

TEST_BINARY="$1"
shift

# ─── Locate kernel tarball ────────────────────────────────────────────

if [ -z "${KERNEL_TARBALL:-}" ]; then
    KERNEL_TARBALL="$(ls -t "$SCRIPT_DIR"/kernel-*.tar.zst 2>/dev/null | head -1 || true)"
fi

if [ -z "$KERNEL_TARBALL" ] || [ ! -f "$KERNEL_TARBALL" ]; then
    echo "ERROR: no kernel tarball found." >&2
    echo "Build one with: .ci/build-kernel.sh" >&2
    echo "Or set KERNEL_TARBALL=/path/to/kernel-*.tar.zst" >&2
    exit 1
fi

# ─── Extract tarball (once, reused across invocations) ────────────────

STAGING="${KERNEL_STAGING:-/tmp/ci-kernel-runner}"

if [ ! -f "$STAGING/boot/bzImage" ]; then
    mkdir -p "$STAGING"
    tar -I zstd -xf "$KERNEL_TARBALL" -C "$STAGING"
fi

BZIMAGE="$STAGING/boot/bzImage"

# ─── Build the in-VM init script ─────────────────────────────────────
#
# We write a small script that:
#   1. Loads USB gadget kernel modules.
#   2. Runs the test binary with the original arguments.
#
# This is passed to vng --exec and runs as the VM's init task.

INIT_SCRIPT="$(mktemp /tmp/ci-vm-init.XXXXXX.sh)"
trap 'rm -f "$INIT_SCRIPT"' EXIT

cat > "$INIT_SCRIPT" <<'INITEOF'
#!/bin/bash
set -euo pipefail

# Load USB gadget modules.
modprobe configfs
modprobe libcomposite
modprobe dummy_hcd is_super_speed=Y

for m in \
    usb_f_fs usb_f_acm usb_f_serial usb_f_ecm usb_f_eem usb_f_ncm \
    usb_f_rndis usb_f_ecm_subset usb_f_hid usb_f_mass_storage \
    usb_f_printer usb_f_midi usb_f_uac2 usb_f_uvc; do
    modprobe "$m" 2>/dev/null || true
done

# Run the test binary.
exec "$@"
INITEOF
chmod +x "$INIT_SCRIPT"

# ─── Launch VM and run the test ───────────────────────────────────────

exec vng \
    --run "$BZIMAGE" \
    --rw \
    --cpus "${VM_CPUS:-$(nproc 2>/dev/null || echo 4)}" \
    --memory "${VM_MEMORY:-4G}" \
    --exec "$INIT_SCRIPT $TEST_BINARY $*"