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

mod volume {
	use std::{
		io::Write,
		process::{Command, Stdio},
	};

	use alsa::mixer::{Mixer, SelemChannelId, SelemId};
	use libnotify::Notification;
	use pulsectl::controllers::{AppControl, DeviceControl, SinkController};

	pub fn add(diff: i8) -> alsa::Result<()> {
		let mixer = Mixer::new("default", false)?;
		let se_id = SelemId::new("Master", 0);
		let selem = mixer.find_selem(&se_id).unwrap();
		let (min, max) = selem.get_playback_volume_range();
		// Current volume
		let volume = selem.get_playback_volume(SelemChannelId::FrontLeft)?;
		// A single percent volume
		let step = (max - min) as f64 * 0.01;
		let new_volume = volume + (step * f64::from(diff)).round() as i64;
		selem.set_playback_volume_all(new_volume.max(min).min(max))?;
		/*let _ = Command::new("pactl")
		.arg("set-sink-volume")
		.arg("@DEFAULT_SINK@")
		.arg(format!("{:+}%", diff))
		.spawn();**/
		Ok(())
	}

	pub fn set_device(controller: &mut SinkController, name: &str) -> Result<(), failure::Error> {
		// Set default device
		match controller.set_default_device(name) {
			Ok(false) => Notification::new("Couldn't set new device", None, None).show()?,
			Err(e) => Notification::new(
				"Error setting default device",
				Some(format!("{:?}", e).as_str()),
				None,
			)
			.show()?,
			_ => (),
		}
		// Change output for all active streams
		for app in controller.list_applications().unwrap() {
			if let Err(e) = controller.move_app_by_name(app.index, name) {
				Notification::new(
					"Error changing sink for applicaiton",
					Some(format!("{:?}", e).as_str()),
					None,
				)
				.show()?;
			}
		}
		Ok(())
	}

	pub fn menu() -> Result<(), failure::Error> {
		let mut controller = SinkController::create()?;
		// Launch device selection dialogue
		let mut cmd = Command::new("zenity")
			.args(&[
				"--list",
				"--text=Choose an audio device",
				"--column=device-id",
				"--column=Device name",
				"--hide-column=1",
				"--width=450",
				"--height=250",
			])
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()?;
		// Write device names to process stdin
		{
			let mut stdin = cmd.stdin.as_mut().unwrap();
			for device in controller.list_devices().unwrap_or_default() {
				writeln!(&mut stdin, "{}", device.name.unwrap_or_default())?;
				writeln!(&mut stdin, "{}", device.description.unwrap_or_default())?;
			}
		}
		// Get process stdout
		let output = cmd.wait_with_output()?;
		let new_device = String::from_utf8_lossy(&output.stdout);
		let new_device = new_device.trim();
		// Set audio device
		if !new_device.is_empty() {
			set_device(&mut controller, new_device)?;
		}
		Ok(())
	}

	/// Toggles whether volume is muted
	pub fn mute() -> alsa::Result<()> {
		let mixer = Mixer::new("default", false)?;
		let se_id = SelemId::new("Master", 0);
		let selem = mixer.find_selem(&se_id).unwrap();

		let muted = selem.get_playback_switch(SelemChannelId::FrontLeft)? == 0;
		selem.set_playback_switch_all(if muted { 1 } else { 0 })?;
		Ok(())
	}

	pub fn icon(vol: u8) -> &'static str {
		match vol {
			0..=29 => "",
			30..=59 => "",
			_ => "",
		}
	}
}

fn main() {
	libnotify::init(env!("CARGO_PKG_NAME")).unwrap();
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
		.add(Periodic::new(Duration::from_secs(2), || {
			// Get the filesystem mounted at root
			let fs = System::new().mount_at(Path::new("/")).unwrap();
			bfmt![
				fg["#cccccc"]
				fmt[" {}", fs.avail.to_string()]
			]
		}))
		// Access point name
		.add(Periodic::new(Duration::from_secs(1), || {
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
			let (name, color) = match connection.split(':').next().unwrap() {
				"" => ("Disconnected", "#BB5555"),
				name => (name, "#99ee99"),
			};
			bfmt![
				fg[color]
				fmt["{} {}{}", icon, name, status]
			]
		}))
		// Load average
		.add(Periodic::new(Duration::from_secs(1), || {
			let load = System::new().load_average().unwrap();
			bfmt![
				fg["#cc9999"]
				fmt[" {:.2}", load.one]
			]
		}))
		// Memory
		.add(Periodic::new(Duration::from_secs(2), || {
			let memory = System::new().memory().unwrap();
			let free = memory.free.as_u64() as f32 / 1_000_000_000.0;
			bfmt![
				fg["#ffc300"]
				fmt[" {:.1} G", free]
			]
		}))
		// Temperature
		.add(Periodic::new(Duration::from_secs(2), || {
			let temp = System::new().cpu_temp().unwrap();
			let icon = match temp as u32 {
				0..=59 => "",
				60..=69 => "",
				70..=79 => "",
				80..=89 => "",
				_ => "",
			};
			bfmt![
				fg["#10ff10"]
				fmt["{} {:.1} °C", icon, temp]
			]
		}))
		// Battery
		.add(Periodic::new(Duration::from_secs(1), move || {
			let charging = match System::new().on_ac_power() {
				Ok(on_ac) => on_ac,
				_ => return bfmt![text[""]],
			};
			let battery = match System::new().battery_life() {
				Ok(battery) => battery,
				_ => return bfmt![text[""]],
			};
			let capacity = (battery.remaining_capacity * 100.0).round() as u8;

			// Send notification if needed
			let battery_warned = battery_warned.clone();
			if capacity <= 10 && !charging {
				if !(battery_warned.load(Ordering::Release)) {
					let notif = Notification::new(
						"Battery level critical",
						Some("Connect to power source immediately"),
						Some("battery-caution"),
					);
					notif.set_urgency(Urgency::Critical);
					notif.show().unwrap();
					battery_warned.store(true, Ordering::Acquire);
				}
			} else {
				battery_warned.store(false, Ordering::Acquire)
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
			bfmt![
				fg[color]
				fmt["{}{} {:.0}%", if charging { "" } else { "" }, icon, capacity]
			]
		}))
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
}
