use super::{Op, finite, resref};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weather {
    Clear,
    Rain,
    Snow,
    Default,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayNight {
    Day,
    Night,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayNightBoth {
    Day,
    Night,
    Both,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicSlot {
    Day,
    Night,
    Battle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioAction {
    Play,
    Stop,
}

pub(super) fn validate(op: &Op) -> Result<(), String> {
    fn color(v: Option<u32>) -> Result<(), String> {
        if v.is_some_and(|v| v > 0xffffff) {
            Err("RGB color must be 0-16777215".into())
        } else {
            Ok(())
        }
    }
    fn fade(v: f32) -> Result<(), String> {
        if !v.is_finite() || !(0.0..=30.0).contains(&v) {
            Err("fade must be 0-30 seconds".into())
        } else {
            Ok(())
        }
    }
    match op {
        Op::SetWeather { .. } => Ok(()),
        Op::SetFog { color: c, amount, fade: f, .. } => {
            color(*c)?;
            fade(*f)?;
            if (c.is_none() && amount.is_none()) || amount.is_some_and(|n| n > 100) {
                Err("Fog requires color or amount 0-100".into())
            } else {
                Ok(())
            }
        }
        Op::SetAreaLight { ambient, diffuse, fade: f, .. } => {
            color(*ambient)?;
            color(*diffuse)?;
            fade(*f)?;
            if ambient.is_none() && diffuse.is_none() {
                Err("Ambient or diffuse required".into())
            } else {
                Ok(())
            }
        }
        Op::SetSkybox { row } => {
            if *row > 65535 {
                Err("Skybox row must be 0-65535".into())
            } else {
                Ok(())
            }
        }
        Op::SetMusic { track, action, .. } => {
            if (track.is_none() && action.is_none()) || track.is_some_and(|n| n > 65535) {
                Err("Music requires track 0-65535 or action".into())
            } else {
                Ok(())
            }
        }
        Op::SetAmbientSound { track, volume, action, .. } => {
            if (track.is_none() && volume.is_none() && action.is_none())
                || track.is_some_and(|n| n > 65535)
                || volume.is_some_and(|n| n > 127)
            {
                Err("Ambient requires track 0-65535, volume 0-127 or action".into())
            } else {
                Ok(())
            }
        }
        Op::SoundAt { resref: r, x, y, z } => {
            resref(r)?;
            finite(&[*x, *y, *z])
        }
        _ => unreachable!(),
    }
}
