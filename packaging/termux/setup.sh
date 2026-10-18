#!/data/data/com.termux/files/usr/bin/sh
# Build ZapFast's terminal client on this phone and supervise Termux OpenSSH.
set -eu

termux_prefix=/data/data/com.termux/files/usr
if [ "${PREFIX:-}" != "$termux_prefix" ] || [ ! -x "$termux_prefix/bin/pkg" ]; then
    printf '%s\n' 'Run this script inside the main Termux app on the Android phone.' >&2
    exit 1
fi
if [ "${HOME:-}" != /data/data/com.termux/files/home ]; then
    printf '%s\n' 'Termux HOME is unexpected; refusing to use another profile.' >&2
    exit 1
fi

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
repo_dir=$(CDPATH='' cd -- "$script_dir/../.." && pwd -P)
if [ ! -f "$repo_dir/Cargo.toml" ]; then
    printf '%s\n' 'Could not find the ZapFast checkout beside this setup script.' >&2
    exit 1
fi

printf '%s\n' 'Installing Termux build tools and the SSH service...'
pkg update -y
pkg install -y git rust clang make pkg-config openssl termux-services
# Install OpenSSH after termux-services so its supervised service is registered.
pkg install -y openssh

printf '%s\n' 'Building zapfast-ssh on this Android phone...'
(
    cd "$repo_dir"
    cargo build --release --locked --no-default-features --features ssh --bin zapfast-ssh
)

binary=$repo_dir/target/release/zapfast-ssh
destination=$PREFIX/bin/zapfast-ssh
staged=$(mktemp "$PREFIX/bin/.zapfast-ssh.XXXXXXXX")
trap 'rm -f "$staged"' 0
trap 'exit 1' 1 2 3 15
install -m 0755 "$binary" "$staged"
mv -f "$staged" "$destination"
trap - 0

# This official Termux profile script starts the service supervisor in the
# current session; later Termux shells start it through their own profile.
services_profile=$PREFIX/etc/profile.d/start-services.sh
if [ ! -f "$services_profile" ]; then
    printf '%s\n' 'termux-services installed without its startup profile; reopen Termux and run: sv-enable sshd' >&2
    exit 1
fi
# The file is provided by the termux-services package installed above.
# shellcheck disable=SC1090
. "$services_profile"

attempt=0
until sv-enable sshd; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 10 ]; then
        printf '%s\n' 'Could not start sshd. Reopen Termux, run sv-enable sshd, then inspect sv status sshd.' >&2
        exit 1
    fi
    sleep 1
done

printf '\n%s\n' 'ZapFast SSH is installed and the Termux SSH service is enabled.'
printf 'Termux user: %s\n' "$(id -un)"
printf '%s\n' 'After adding your computer public key to ~/.ssh/authorized_keys, connect from that computer:'
printf '  ssh -t -p 8022 %s@PHONE_WIFI_IP zapfast-ssh\n' "$(id -un)"
printf '%s\n' 'The WhatsApp client connects only while that SSH command is running.'
