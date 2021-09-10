# bars

It's my statusbar! It's made with (my [fork] of) [unixbar].

[fork]: https://github.com/agraven/unixbar
[unixbar]: https://github.com/unrelentingtech/unixbar

## Building

Building requires the following libraries:
* dbus
* libnotify
* alsa
* gdk-pixbuf2

Fedora dependencies:
`sudo dnf install alsa-lib-devel dbus-devel gdk-pixbuf2-devel libnotify-devel libxkbcommon-devel libxcb-devel pulseaudio-libs-devel`

Ubuntu dependencies:
`sudo apt-get install libdbus-1-dev libnotify-dev libgdk-pixbuf2.0-dev libasound2-dev libxcb-xkb-dev libpulse-dev`

Some might be missing from this list, please notify me if so
