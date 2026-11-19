//! Preset files from the UI (FL: channel presets, mixer track states, effect
//! presets): save and load dialogs, and opening a preset from the browser.

use super::{App, Tone};
use keyframexl::preset::{self, ChannelPreset, FxPreset, Kind, MixerPreset};
use std::path::{Path, PathBuf};

/// Where a loaded preset goes.
#[derive(Clone, Copy, Debug)]
pub enum Target {
    /// Replace this channel's instrument.
    Channel(u64),
    /// A new channel (browser click, drop on the rack).
    NewChannel,
    Mixer(usize),
    /// Slot `k` of track `t` (appended when `k` is past the end).
    Fx(usize, usize),
    /// This synth channel's patch, or a new synth channel.
    Synth(Option<u64>),
}

/// Saved synth presets (`Presets/Synth`), sorted by name.
pub(crate) fn user_synth_presets() -> Vec<(PathBuf, preset::SynthPreset)> {
    let Some(dir) = preset::dir(Kind::Synth) else {
        return vec![];
    };
    let mut v: Vec<(PathBuf, preset::SynthPreset)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| Kind::of_path(p) == Some(Kind::Synth))
        .filter_map(|p| {
            preset::load::<preset::SynthPreset>(Kind::Synth, &p)
                .ok()
                .filter(|s| s.patch.validate().is_ok())
                .map(|s| (p, s))
        })
        .collect();
    v.sort_by_key(|(_, s)| s.name.to_lowercase());
    v
}

/// The saved synth presets, read once and again after a save (egui memory).
pub(crate) fn cached_synth_presets(
    ctx: &eframe::egui::Context,
) -> std::sync::Arc<Vec<(PathBuf, preset::SynthPreset)>> {
    let id = eframe::egui::Id::new("tide-user-presets");
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || std::sync::Arc::new(user_synth_presets()))
            .clone()
    })
}

pub(crate) fn forget_synth_presets(ctx: &eframe::egui::Context) {
    ctx.data_mut(|d| {
        d.remove::<std::sync::Arc<Vec<(PathBuf, preset::SynthPreset)>>>(eframe::egui::Id::new(
            "tide-user-presets",
        ))
    });
}

/// Our presets made from the plugin at `plugin_path`, sorted by name.
pub(crate) fn plugin_presets(kind: Kind, plugin_path: &str) -> Vec<PathBuf> {
    let Some(dir) = preset::dir(kind) else {
        return vec![];
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| Kind::of_path(p) == Some(kind))
        .filter(|p| match kind {
            Kind::Channel => preset::load::<ChannelPreset>(kind, p)
                .ok()
                .and_then(|c| c.kind_plugin_path())
                .is_some_and(|x| x == plugin_path),
            Kind::Fx => preset::load::<FxPreset>(kind, p)
                .ok()
                .and_then(|f| f.slot.plugin.map(|r| r.path))
                .is_some_and(|x| x == plugin_path),
            Kind::Mixer | Kind::Synth => false,
        })
        .collect();
    v.sort();
    v
}

impl App {
    fn preset_save_dialog(kind: Kind, name: &str) -> Option<PathBuf> {
        let dir = preset::dir(kind);
        if let Some(d) = &dir {
            let _ = std::fs::create_dir_all(d);
        }
        let mut dialog = rfd::FileDialog::new()
            .add_filter(kind.label(), &[kind.ext()])
            .set_file_name(preset::file_name(name, kind));
        if let Some(d) = dir {
            dialog = dialog.set_directory(d);
        }
        let path = dialog.save_file()?;
        // A name typed without the extension still gets it.
        Some(if Kind::of_path(&path) == Some(kind) {
            path
        } else {
            path.with_extension(kind.ext())
        })
    }

    fn preset_open_dialog(kind: Kind) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().add_filter(kind.label(), &[kind.ext()]);
        if let Some(d) = preset::dir(kind).filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(d);
        }
        dialog.pick_file()
    }

    fn report_save(&mut self, kind: Kind, path: &Path, r: Result<(), String>) {
        match r {
            Ok(()) => self.say(
                Tone::Done,
                format!("{} saved: {}", kind.label(), path.display()),
            ),
            Err(e) => self.say(
                Tone::Bad,
                format!("Could not save the {}: {e}", kind.label().to_lowercase()),
            ),
        }
    }

    pub(crate) fn save_channel_preset(&mut self, id: u64) {
        self.collect_plugin_states();
        let Some(inst) = self.song.instruments.iter().find(|i| i.id == id) else {
            return;
        };
        let data = ChannelPreset::of(inst);
        let Some(path) = Self::preset_save_dialog(Kind::Channel, &inst.name) else {
            return;
        };
        let r = preset::save(Kind::Channel, &data, &path);
        self.report_save(Kind::Channel, &path, r);
        self.plugins.preset_list.clear();
    }

    pub(crate) fn save_mixer_preset(&mut self, t: usize) {
        self.collect_plugin_states();
        let Some(track) = self.song.mixer.get(t) else {
            return;
        };
        let data = MixerPreset::of(track);
        let name = if track.name.is_empty() {
            format!("Insert {t}")
        } else {
            track.name.clone()
        };
        let Some(path) = Self::preset_save_dialog(Kind::Mixer, &name) else {
            return;
        };
        let r = preset::save(Kind::Mixer, &data, &path);
        self.report_save(Kind::Mixer, &path, r);
    }

    pub(crate) fn save_fx_preset(&mut self, t: usize, k: usize) {
        self.collect_plugin_states();
        let Some(slot) = self.song.mixer.get(t).and_then(|m| m.fx.get(k)) else {
            return;
        };
        let data = FxPreset::of(slot);
        let name = slot.label().to_string();
        let Some(path) = Self::preset_save_dialog(Kind::Fx, &name) else {
            return;
        };
        let r = preset::save(Kind::Fx, &data, &path);
        self.report_save(Kind::Fx, &path, r);
        self.plugins.preset_list.clear();
    }

    /// The channel a synth preset goes to: the selected channel when it is a synth.
    pub(crate) fn selected_synth(&self) -> Option<u64> {
        self.song
            .instruments
            .iter()
            .find(|i| {
                i.id == self.instrument
                    && matches!(i.kind, keyframexl::sequence::InstrumentKind::Synth { .. })
            })
            .map(|i| i.id)
    }

    /// Put `patch` on synth channel `at`, or on a new synth channel named
    /// after the preset (opened in its editor). One undo step; the caller
    /// publishes. Returns the status line.
    pub(crate) fn apply_synth_patch(
        &mut self,
        at: Option<u64>,
        name: &str,
        patch: keyframexl::synth::SynthPatch,
    ) -> String {
        use keyframexl::sequence::InstrumentKind;
        self.checkpoint();
        if let Some(id) = at
            && let Some(inst) = self.song.instruments.iter_mut().find(|i| i.id == id)
        {
            inst.kind = InstrumentKind::Synth { patch };
            return format!("{name} on {}. Ctrl+Z undoes.", inst.name);
        }
        let channel = self.song.free_name(name);
        let id = self
            .song
            .add_channel(&channel, InstrumentKind::Synth { patch });
        self.instrument = id;
        self.win(super::Cmd::Show(super::Tab::Instrument { id }));
        format!("{channel} added with the {name} patch. Ctrl+Z undoes.")
    }

    /// Save synth channel `id`'s patch as a preset (`Presets/Synth`).
    pub(crate) fn save_synth_preset(&mut self, ctx: &eframe::egui::Context, id: u64, name: &str) {
        let Some(keyframexl::sequence::InstrumentKind::Synth { patch }) = self
            .song
            .instruments
            .iter()
            .find(|i| i.id == id)
            .map(|i| &i.kind)
        else {
            return;
        };
        let patch = patch.clone();
        let Some(path) = Self::preset_save_dialog(Kind::Synth, name) else {
            return;
        };
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_owned());
        let r = preset::save(Kind::Synth, &preset::SynthPreset { name, patch }, &path);
        self.report_save(Kind::Synth, &path, r);
        forget_synth_presets(ctx);
    }

    /// Pick a file of the kind `target` takes, then load it there.
    pub(crate) fn load_preset_dialog_for(&mut self, target: Target) {
        let kind = match target {
            Target::Channel(_) | Target::NewChannel => Kind::Channel,
            Target::Mixer(_) => Kind::Mixer,
            Target::Fx(..) => Kind::Fx,
            Target::Synth(_) => Kind::Synth,
        };
        if let Some(path) = Self::preset_open_dialog(kind) {
            self.load_preset_into(&path, target);
        }
    }

    /// A preset file from the browser: a channel preset becomes a new channel,
    /// a mixer preset goes onto the selected track, an effect preset into the
    /// selected track's first free slot.
    pub(crate) fn open_preset_file(&mut self, path: &Path) {
        let target = match Kind::of_path(path) {
            Some(Kind::Channel) => Target::NewChannel,
            Some(Kind::Mixer) => Target::Mixer(self.mix_sel),
            Some(Kind::Fx) => Target::Fx(
                self.mix_sel,
                self.song
                    .mixer
                    .get(self.mix_sel)
                    .map(|m| m.fx.len())
                    .unwrap_or(0),
            ),
            Some(Kind::Synth) => Target::Synth(self.selected_synth()),
            None => {
                self.say(Tone::Bad, format!("{} is not a preset", path.display()));
                return;
            }
        };
        self.load_preset_into(path, target);
    }

    pub(crate) fn load_preset_into(&mut self, path: &Path, target: Target) {
        let file = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let result: Result<String, String> = match target {
            Target::Channel(id) => preset::load::<ChannelPreset>(Kind::Channel, path)
                .and_then(|p| {
                    p.validate().map_err(str::to_owned)?;
                    Ok(p)
                })
                .map(|p| {
                    self.collect_plugin_states();
                    self.checkpoint();
                    p.apply(&mut self.song, id);
                    format!("Channel preset {file} loaded")
                }),
            Target::NewChannel => preset::load::<ChannelPreset>(Kind::Channel, path)
                .and_then(|p| {
                    p.validate().map_err(str::to_owned)?;
                    Ok(p)
                })
                .map(|p| {
                    self.checkpoint();
                    let id = p.add(&mut self.song);
                    self.instrument = id;
                    format!("Channel {file} added")
                }),
            Target::Mixer(t) => preset::load::<MixerPreset>(Kind::Mixer, path).map(|p| {
                self.collect_plugin_states();
                self.checkpoint();
                p.apply(&mut self.song, t);
                format!("Mixer preset {file} on {}", self.mixer_name(t))
            }),
            Target::Synth(at) => {
                preset::load::<preset::SynthPreset>(Kind::Synth, path).and_then(|p| {
                    p.patch.validate().map_err(str::to_owned)?;
                    Ok(self.apply_synth_patch(at, &p.name, p.patch))
                })
            }
            Target::Fx(t, k) => preset::load::<FxPreset>(Kind::Fx, path).map(|p| {
                self.collect_plugin_states();
                self.checkpoint();
                p.apply(&mut self.song, t, k);
                format!("Effect preset {file} on {}", self.mixer_name(t))
            }),
        };
        match result {
            Ok(msg) => {
                self.publish();
                self.say(Tone::Done, msg);
            }
            Err(e) => self.say(Tone::Bad, format!("{file}: {e}")),
        }
    }
}

#[cfg(test)]
mod synth_preset_tests {
    use super::*;
    use keyframexl::sequence::{InstrumentKind, Song};
    use keyframexl::synth::SynthPatch;

    fn patch_of(app: &App, id: u64) -> SynthPatch {
        match &app
            .song
            .instruments
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .kind
        {
            InstrumentKind::Synth { patch } => patch.clone(),
            _ => panic!("not a synth"),
        }
    }

    #[test]
    fn a_synth_preset_file_round_trips_and_refuses_other_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(preset::file_name("Wobble edit", Kind::Synth));
        assert!(path.to_string_lossy().ends_with("Wobble edit.kfxsynth"));
        let saved = preset::SynthPreset {
            name: "Wobble edit".into(),
            patch: SynthPatch::factory(12),
        };
        preset::save(Kind::Synth, &saved, &path).unwrap();
        let back: preset::SynthPreset = preset::load(Kind::Synth, &path).unwrap();
        assert_eq!(back.patch, saved.patch);
        assert_eq!(Kind::of_path(&path), Some(Kind::Synth));
        assert!(
            preset::load::<ChannelPreset>(Kind::Channel, &path).is_err(),
            "a synth preset is not a channel preset"
        );
    }

    #[test]
    fn a_synth_patch_goes_to_the_selected_synth_or_a_new_channel_with_one_undo() {
        let mut app = App::with_song(Song::default(), None);
        app.instrument = 1; // Kick: not a synth.
        assert_eq!(app.selected_synth(), None);
        let before = app.song.instruments.len();
        let msg = app.apply_synth_patch(None, "Glass Keys", SynthPatch::factory(11));
        assert!(msg.contains("Glass Keys added"), "{msg}");
        assert_eq!(app.song.instruments.len(), before + 1);
        let id = app.instrument;
        assert_eq!(app.selected_synth(), Some(id), "the new synth is selected");
        assert_eq!(patch_of(&app, id), SynthPatch::factory(11));
        app.apply_synth_patch(Some(id), "Grit Bass", SynthPatch::factory(10));
        assert_eq!(patch_of(&app, id), SynthPatch::factory(10));
        assert_eq!(app.song.instruments.len(), before + 1, "replaced in place");
        app.undo();
        assert_eq!(
            patch_of(&app, id),
            SynthPatch::factory(11),
            "one undo step back"
        );
    }
}
