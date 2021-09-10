// bars - Copyright © Amanda Graven 2021
//
// Licensed under the EUPL, Version 1.2 or – as soon they will be approved by
// the European Commission - subsequent versions of the EUPL (the "Licence");
// You may not use this work except in compliance with the Licence.
// You may obtain a copy of the Licence at:
//
// https://joinup.ec.europa.eu/software/page/eupl5
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the Licence is distributed on an "AS IS" basis, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
// Licence for the specific language governing permissions and limitations under
// the Licence.

extern crate alsa;
extern crate failure;
extern crate systemstat;
extern crate unixbar;

mod volume;

use std::{
	io::ErrorKind as IoErrorKind,
	path::Path,
	process::Command,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};

use libnotify::{Notification, Urgency};
use systemstat::{Platform, System};
use unixbar::{
	bfmt,
	format::{ClickAction, Format, I3BarFormatter, MouseButton},
	widget::{
		backlight::Backlight, music::MusicControl, DateTime, MPRISMusic, Music, Periodic, Text,
		Volume, ALSA,
	},
	Duration, UnixBar,
};
use anyhow::{anyhow, Result};


fn catch<F: FnMut() -> Result<Format, anyhow::Error>>(mut closure: F) -> Format {
    match closure() {
        Ok(fmt) => fmt,
        Err(e) => bfmt![fg["#ff5555"] fmt["{}", e.to_string()]]
    }
}

fn main() -> Result<()> {
	libnotify::init(env!("CARGO_PKG_NAME")).map_err(|e| anyhow!(e))?;
	let battery_warned = Arc::new(AtomicBool::new(false));
	// The structure representing the bar to generate
	let formatter = I3BarFormatter::new();
	UnixBar::new(formatter)
		// Media play funtions
		.register_fn("mus_toggle", || MPRISMusic::new().play_pause())
		.register_fn("mus_prev", || MPRISMusic::new().prev())
		.register_fn("mus_next", || MPRISMusic::new().next())
		// Media player widget
		.add(Music::new(MPRISMusic::new(), |song| {
			// Playing or paused
			if let Some(playback) = song.playback {
				let icon = match playback.playing {
					true => "",
					false => "",
				};
				bfmt![
					click[MouseButton::Left => fn "mus_prev"]
					click[MouseButton::Middle => fn "mus_toggle"]
					click[MouseButton::Right => fn "mus_next"]
					fg["#9090ff"]
					fmt["{}  {} - {}", icon, song.artist, song.title]
				]
			} else {
				bfmt![
					//fg["#9090ff"]
					//fmt[" Stopped"]
					text[""]
				]
			}
		}))
		// Volume functions
		.register_fn("vol_up", || volume::add(5).unwrap_or(()))
		.register_fn("vol_down", || volume::add(-5).unwrap_or(()))
		.register_fn("vol_mute", || volume::mute().unwrap_or(()))
		.register_fn("device_menu", || volume::menu().unwrap_or(()))
		// Volume widget
		.add(Volume::new(ALSA::new(), |volume| {
			bfmt![
				click[MouseButton::ScrollDown => fn "vol_down"]
				click[MouseButton::ScrollUp => fn "vol_up"]
				click[MouseButton::Middle => fn "vol_mute"]
				click[MouseButton::Left => fn "device_menu"]
				fg["#9090ff"]
				fmt["{}", match volume.muted {
					true => String::from("🔇 MUTE"),
					false => {
						let vol = volume.volume * 100.0;
						format!("{} {:.0}%", volume::icon(vol as u8), vol)
					}
				}]
			]
		}))
		// IBus keyboard layout
		.add(Periodic::new(Duration::from_secs(1), || {
			let output = match Command::new("ibus").arg("engine").output() {
				Ok(out) => out,
				_ => return bfmt![text[""]],
			};
			let string = String::from_utf8_lossy(&output.stdout);
			let layout = string.split(':').nth(1).unwrap_or("N/A");
			bfmt![fmt["⌨ {}", layout]]
		}))
		// Disk space
		.add(Periodic::new(Duration::from_secs(2), || catch(|| {
			// Get the filesystem mounted at root
			let fs = System::new().mount_at(Path::new("/"))?;
			Ok(bfmt![
				fg["#cccccc"]
				fmt[" {}", fs.avail.to_string()]
			])
		})))
		// Access point name
		.add(Periodic::new(Duration::from_secs(1), || catch(|| {
			let nmcli = |args: &[&str]| -> String {
				Command::new("nmcli")
					.args(args)
					.output()
					.map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
					.unwrap_or_default()
			};
			let connection = nmcli(&["--terse", "connection", "show", "--active"]);
			let connectivity = nmcli(&["networking", "connectivity", "check"]);
			let status = match connectivity.trim_end() {
				"full" => "",
				"portal" | "limited" | "none" => "!",
				"unknown" | "" => "?",
				_ => "?",
			};
			let icon = connection
				.split(':')
				.nth(2)
				.map(|kind| match kind {
					"802-3-ethernet" => "",
					"802-11-wireless" => "",
					_ => "﹖",
				})
				.unwrap_or("﹖");
			let (name, color) = match connection.split(':').next().ok_or_else(|| anyhow!("nmcli returned unexpected data!"))? {
				"" => ("Disconnected", "#BB5555"),
				name => (name, "#99ee99"),
			};
			Ok(bfmt![
				fg[color]
				fmt["{} {}{}", icon, name, status]
			])
		})))
		// Load average
		.add(Periodic::new(Duration::from_secs(1), || catch (|| {
			let load = System::new().load_average()?;
			Ok(bfmt![
				fg["#cc9999"]
				fmt[" {:.2}", load.one]
			])
		})))
		// Memory
		.add(Periodic::new(Duration::from_secs(2), || catch(|| {
			let memory = System::new().memory()?;
			let free = memory.free.as_u64() as f32 / 1_000_000_000.0;
			Ok(bfmt![
				fg["#ffc300"]
				fmt[" {:.1} G", free]
			])
		})))
		// Temperature
		.add(Periodic::new(Duration::from_secs(2), || catch(|| {
			let temp = System::new().cpu_temp()?;
			let icon = match temp as u32 {
				0..=59 => "",
				60..=69 => "",
				70..=79 => "",
				80..=89 => "",
				_ => "",
			};
			Ok(bfmt![
				fg["#10ff10"]
				fmt["{} {:.1} °C", icon, temp]
			])
		})))
		// Battery
		.add(Periodic::new(Duration::from_secs(1), move || catch(|| {
			let charging = match System::new().on_ac_power() {
				Ok(on_ac) => on_ac,
				_ => return Ok(bfmt![text[""]]),
			};
			let battery = match System::new().battery_life() {
				Ok(battery) => battery,
				_ => return Ok(bfmt![text[""]]),
			};
			let capacity = (battery.remaining_capacity * 100.0).round() as u8;

			// Send notification if needed
			let battery_warned = battery_warned.clone();
			if capacity <= 10 && !charging {
				if !(battery_warned.load(Ordering::Acquire)) {
					let notif = Notification::new(
						"Battery level critical",
						Some("Connect to power source immediately"),
						Some("battery-caution"),
					);
					notif.set_urgency(Urgency::Critical);
					notif.show()?;
					battery_warned.store(true, Ordering::Release);
				}
			} else {
				battery_warned.store(false, Ordering::Release)
			}

			let (icon, color) = match capacity {
				0..=19 => ("", "#FF4000"),
				20..=39 => ("", "#FFAE00"),
				40..=59 => ("", "#FFF600"),
				60..=79 => ("", "#A8FF00"),
				80..=99 => ("", "#50FF00"),
				100 if charging => ("", "#50FF00"),
				_ => ("", "#50FF00"),
			};
			Ok(bfmt![
				fg[color]
				fmt["{}{} {:.0}%", if charging { "" } else { "" }, icon, capacity]
			])
		})))
		// Brightness
		.register_fn("bright_up", || Backlight::adjust(0.05).unwrap_or(()))
		.register_fn("bright_down", || Backlight::adjust(-0.05).unwrap_or(()))
		.add(Backlight::new(|| match Backlight::get() {
			Ok(brightness) => bfmt![
				click[MouseButton::ScrollUp => fn "bright_up"]
				click[MouseButton::ScrollDown => fn "bright_down"]
				fg["#ffff55"]
				fmt["☀ {:.0}%", brightness * 100.0]
			],
			Err(e) if e.kind() == IoErrorKind::NotFound => bfmt![text[""]],
			Err(e) => bfmt![fmt["ERROR: {}", e]],
		}))
		// Time
		.add(DateTime::new(" %d/%m %H:%M"))
		// Flair
		.add(Text::new(bfmt![text["(◕ᴗ◕✿)"]]))
		.run();
	//libnotify::uninit();
Ok(())
}
