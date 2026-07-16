#!/bin/sh
set -eu

version="${1:-0.1.0}"
arch="${2:-$(dpkg --print-architecture)}"
root="target/debian/woc-widget-cosmic-applet_${version}_${arch}"
desktop="io.github.fernandox7.wocplayercount.cosmic-applet.desktop"

cargo build --release -p woc-cosmic-applet
install -Dm0755 target/release/woc-cosmic-applet "$root/usr/bin/woc-cosmic-applet"
install -Dm0644 "woc-cosmic-applet/resources/$desktop" \
  "$root/usr/share/applications/$desktop"
install -Dm0755 -d "$root/DEBIAN"
sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@ARCH@/$arch/g" \
  woc-cosmic-applet/resources/debian-control.in > "$root/DEBIAN/control"
dpkg-deb --build --root-owner-group "$root"
