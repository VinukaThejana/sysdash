#!/usr/bin/env bash
# Puts `sysdash tui` on tty1 in place of the login prompt. Run with sudo from your own
# account (the view runs as you, from ~/.local/bin/sysdash):
#   sudo bash deploy/console.sh            install
#   sudo bash deploy/console.sh --undo     put the login prompt back
#
# Install also switches the console font to Terminus (the default kernel font lacks the
# TUI's thick lines and bar characters); the previous /etc/vconsole.conf is kept as a backup.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
unit=/etc/systemd/system/sysdash-console.service
vconsole=/etc/vconsole.conf
font=ter-v24n # 24px tall: 160 x 45 on a 1080p screen, readable from a sofa

if [[ ${1:-} == --undo ]]; then
  systemctl disable --now sysdash-console.service || true
  rm -f "$unit"
  systemctl unmask getty@tty1.service autovt@tty1.service
  systemctl daemon-reload
  systemctl enable --now getty@tty1.service
  if [[ -f $vconsole.bak-sysdash ]]; then
    cp "$vconsole.bak-sysdash" "$vconsole"
    systemctl restart systemd-vconsole-setup.service || true
  fi
  echo "tty1 shows the login prompt again"
  exit 0
fi

user=${SUDO_USER:?run this with sudo from the account sysdash runs as}
home=$(getent passwd "$user" | cut -d: -f6)
uid=$(id -u "$user")
if [[ ! -x $home/.local/bin/sysdash ]]; then
  echo "install sysdash first: $home/.local/bin/sysdash is missing" >&2
  exit 1
fi

# Terminus' package name differs per distro; the font file is what matters.
if command -v pacman >/dev/null; then
  pacman -S --needed --noconfirm terminus-font || true
elif command -v apt-get >/dev/null; then
  apt-get install -y console-terminus || true
elif command -v dnf >/dev/null; then
  dnf install -y terminus-fonts-console || true
fi
has_font=$(find /usr/share/kbd/consolefonts /usr/lib/kbd/consolefonts /usr/share/consolefonts \
  -name "$font.psf*" 2>/dev/null | head -1)

if [[ -z $has_font ]]; then
  echo "warning: console font $font not found; keeping the current font (lines may look rough)" >&2
elif ! grep -qs "^FONT=$font$" "$vconsole"; then
  [[ -f $vconsole ]] && cp "$vconsole" "$vconsole.bak-sysdash"
  if grep -qs '^FONT=' "$vconsole"; then
    sed -i "s/^FONT=.*/FONT=$font/" "$vconsole"
  else
    echo "FONT=$font" >>"$vconsole"
  fi
fi

sed -e "s|@USER@|$user|g" -e "s|@UID@|$uid|g" -e "s|@HOME@|$home|g" "$here/sysdash-console.service" |
  install -Dm644 /dev/stdin "$unit"
# The view lists failing user services through the user bus, which needs lingering.
loginctl enable-linger "$user"
# logind would otherwise start a getty on tty1 again; masking blocks both names for it.
systemctl mask getty@tty1.service autovt@tty1.service
systemctl stop getty@tty1.service || true
systemctl daemon-reload
systemctl restart systemd-vconsole-setup.service || true
systemctl enable --now sysdash-console.service
echo "tty1 now shows sysdash (undo with: sudo bash $0 --undo)"
