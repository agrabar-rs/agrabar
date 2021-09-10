use std::{
    io::Write,
    process::{Command, Stdio},
};

use alsa::mixer::{Mixer, SelemChannelId, SelemId};
use libnotify::Notification;
use pulsectl::controllers::{AppControl, DeviceControl, SinkController};
use anyhow::anyhow;

pub fn add(diff: i8) -> anyhow::Result<()> {
    let mixer = Mixer::new("default", false)?;
    let se_id = SelemId::new("Master", 0);
    let selem = mixer.find_selem(&se_id).ok_or_else(|| anyhow!("Could not find alsa selem"))?;
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

pub fn set_device(controller: &mut SinkController, name: &str) -> anyhow::Result<()> {
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
    for app in controller.list_applications()? {
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

pub fn menu() -> Result<(), anyhow::Error> {
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

