//! UI localization: English and Ukrainian string tables, CLDR plurals,
//! placeholder formatting, and system locale detection.

#![allow(dead_code)]

mod en;
mod plural;
mod uk;

pub use en::EN;
#[cfg(test)]
pub use plural::Plural;
pub use plural::{PluralStr, fill};
pub use uk::UK;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Ukrainian,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Ukrainian];

    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Ukrainian => "Українська",
        }
    }

    /// Parse a POSIX locale tag (`uk_UA.UTF-8`, `uk-UA`, `uk`, `uk:en`, …).
    /// Returns Ukrainian for `uk*`, falling back to English for anything else.
    pub fn from_locale(locale: &str) -> Self {
        let first = locale.split(':').next().unwrap_or("").trim();
        let lang = first
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if lang == "uk" {
            Self::Ukrainian
        } else {
            Self::English
        }
    }

    /// Detect the user's preferred language from standard POSIX locale env vars,
    /// falling back to English.
    pub fn from_system() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(val) = std::env::var(var) {
                let trimmed = val.trim();
                if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" {
                    return Self::from_locale(trimmed);
                }
            }
        }
        Self::English
    }

    /// Optional explicit override via `KFXL_LANG=en|uk` (useful for captures and testing).
    pub fn from_env_override() -> Option<Self> {
        let raw = std::env::var("KFXL_LANG").ok()?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" => Some(Self::English),
            "uk" | "ua" | "ukr" | "ukrainian" => Some(Self::Ukrainian),
            _ => None,
        }
    }
}

const CTX_LANG_ID: &str = "kfxl-ui-language";

/// Store the active UI language on the egui context so shared widget helpers
/// (such as `kit::auto_menu_item` or knob context menus) can read it without
/// global mutable state races across parallel tests.
pub fn set_ctx_lang(ctx: &eframe::egui::Context, lang: Language) {
    ctx.data_mut(|d| d.insert_temp(eframe::egui::Id::new(CTX_LANG_ID), lang));
}

/// Active UI language stored on the egui context (defaults to English).
pub fn ctx_lang(ctx: &eframe::egui::Context) -> Language {
    ctx.data(|d| d.get_temp::<Language>(eframe::egui::Id::new(CTX_LANG_ID)))
        .unwrap_or(Language::English)
}

/// String table for the active UI language on the egui context.
pub fn ctx_tr(ctx: &eframe::egui::Context) -> &'static Strings {
    strings(ctx_lang(ctx))
}

macro_rules! define_strings {
    (
        strings: { $($field:ident),* $(,)? },
        plurals: { $($pfield:ident),* $(,)? },
    ) => {
        /// One complete set of UI strings for a language.
        pub struct Strings {
            pub lang: Language,
            $(pub $field: &'static str,)*
            $(pub $pfield: PluralStr,)*
        }

        #[cfg(test)]
        impl Strings {
            pub fn entries(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((stringify!($field), self.$field),)*]
            }

            pub fn plural_entries(&self) -> Vec<(&'static str, &PluralStr)> {
                vec![$((stringify!($pfield), &self.$pfield),)*]
            }
        }
    };
}

define_strings! {
    strings: {
        // Common actions
        common_close, common_cancel, common_rename, common_apply,
        // Top menu
        menu_file, menu_edit, menu_add, menu_patterns, menu_view, menu_options, menu_tools, menu_help,
        // Window title bar buttons
        win_minimise, win_restore, win_maximise,
        // File menu
        file_new, file_open, file_open_recent, file_save, file_save_as, file_save_new_version,
        file_project_info, file_export, file_import, file_import_dwp,
        file_import_bundle, file_export_bundle, file_import_flp, file_import_midi, file_export_audio,
        file_export_midi, file_quit,
        // Edit menu
        edit_undo, edit_redo, edit_nothing_undo, edit_nothing_redo,
        edit_cut_notes, edit_copy_notes, edit_paste_notes,
        edit_select_notes_first, edit_copy_notes_first,
        // Add menu
        add_channel, add_effect_mixer, add_plugin_instrument, add_pattern,
        // Patterns menu
        pat_find_next_empty, pat_rename, pat_colour, pat_colour_by_id,
        pat_colour_reset_status, pat_colour_set_status, pat_clone, pat_delete, pat_last_stays,
        // View menu
        view_follow_playhead, view_deck_monitor, view_workspace, view_reset_workspace,
        view_close_all, view_theme, view_ui_scale, view_larger, view_largest,
        view_smaller, view_smallest,
        // Options menu
        opt_audio, opt_midi, opt_plugins, opt_general, opt_metronome, opt_typing_piano,
        opt_record_auto, opt_record_input, opt_song_mode,
        // Tools menu
        tools_last_tweaked, tools_last_tweaked_disabled, tools_create_auto, tools_create_auto_tip,
        tools_link_ctrl, tools_link_ctrl_tip, tools_copy_value, tools_paste_value,
        tools_paste_value_disabled, tools_reset_default, tools_multilink,
        tools_riff_machine, tools_riff_machine_tip, tools_tap_tempo, tools_panic,
        // Help menu & dialogs
        help_welcome, help_demo_song, help_shortcuts, help_manual, help_check_updates, help_about,
        shortcuts_filter_hint, shortcuts_full_manual,
        about_audio_buf, about_audio_default, about_version, about_desc,
        rename_pattern_title,
        // Manual chrome
        manual_search_hint, manual_no_match, manual_open_online, manual_open_online_tip,
        manual_prev, manual_next, manual_open_browser_err,
        // Window titles & workspace
        win_playlist, win_rack, win_roll, win_instrument, win_mixer, win_riff, win_browser,
        ws_arrange, ws_compose, ws_edit, status_ws_switched, status_ws_reset,
        status_layout_not_saved, ws_float_window, ws_dock_window,
        inst_kind_sampler, inst_kind_plugin, inst_kind_synth, inst_kind_vocal,
        inst_kind_forge, inst_kind_drum,
        // Deck / transport / shell
        deck_pattern, deck_song, deck_playlist, deck_rack, deck_roll, deck_mixer, deck_browser,
        deck_win_open_suffix, deck_browser_hint, deck_tap, deck_tap_hint,
        deck_snap_label, deck_snap_hint, deck_prev_pattern, deck_pattern_hint,
        deck_next_pattern, deck_new_pattern,
        deck_cpu_voices, deck_cpu_voices_hint, deck_no_audio, deck_no_audio_hint,
        deck_overloads_tip, deck_scope_tip, deck_spectrum_tip,
        deck_metronome, deck_metronome_on_tip, deck_metronome_off_tip,
        deck_count_in, deck_count_in_every_play, deck_count_in_recording,
        deck_count_in_on_tip, deck_count_in_off_tip, deck_count_in_on_status,
        deck_count_in_off_status, deck_count_in_menu_rec, deck_count_in_menu_always,
        deck_wait_input, deck_wait_input_on_tip, deck_wait_input_off_tip,
        deck_wait_input_on_status, deck_wait_input_off_status,
        deck_play, deck_pause, deck_pause_tip, deck_play_tip, deck_stop, deck_stop_tip,
        deck_record, deck_rec_on_tip, deck_rec_off_tip,
        lcd_bar, lcd_step,
        deck_tempo_up, deck_tempo_down, deck_tempo_step_tip, deck_tempo_field_tip,
        untitled, unsaved,
        // Tools & snap labels
        tool_draw, tool_paint, tool_delete, tool_mute, tool_slice, tool_select, tool_zoom, tool_slide,
        tool_draw_hint, tool_paint_hint, tool_delete_hint, tool_mute_hint,
        tool_slice_hint, tool_select_hint, tool_zoom_hint, tool_slide_hint,
        snap_bar, snap_off,
        // Deck monitor & theme mode labels
        monitor_off, monitor_scope, monitor_spectrum,
        theme_dark, theme_light, theme_system,
        // Shell status messages, hints, empty dock & drop overlay
        status_no_audio_init, status_wine_conflict, status_undone, status_redone,
        status_pattern_added, status_pattern_empty_selected, status_pattern_copy_added,
        status_pattern_last_stays, status_pattern_renamed, status_all_sound_stopped,
        status_demo_voice_err, status_demo_loaded,
        status_tempo_set, status_tempo_range_err, status_tempo_event_set,
        status_playhead_at, status_start_cancelled, status_waiting_input,
        status_count_in_rec, status_count_in_play, status_rec_from_first_note,
        status_tap_progress, status_tempo_from_taps,
        dlg_filter_kfxl, status_saved, status_not_saved, status_new_project,
        dlg_filter_bundle, status_bundled, status_bundle_export_failed,
        status_bundle_unpacked, status_bundle_open_failed,
        dlg_filter_open_all, status_opened, status_cannot_open, dlg_filter_flp,
        status_started_template, status_loaded_tide_preset, status_loaded_forge_recipe,
        flp_import_comments_header, status_flp_imported, status_cannot_import,
        flp_note_needs_vsti, flp_note_not_moved, flp_note_fx_missing,
        flp_note_inst_missing, flp_note_auto_missing,
        status_fl_render_err, status_fl_rendered,
        status_preset_loaded, status_not_loaded, status_preset_not_loaded,
        status_tool_selected, status_midi_learn_prompt,
        status_no_audio_pill, status_update_restart_pill, status_update_installing_pill,
        status_update_avail_pill, status_update_avail_tip,
        hint_rack, hint_roll, hint_playlist, hint_mixer, hint_instrument,
        hint_riff, hint_settings, hint_empty,
        win_header_menu_tip, win_dock, win_detach_float,
        empty_all_closed, empty_keys_hint, empty_restore_layout,
        drop_dwp_into, drop_sample_into, drop_open_project, drop_import_flp,
        drop_midi_notes, drop_any_file,
        // Settings
        settings_title, tab_audio, tab_midi, tab_plugins, tab_general, setting_language,
        setting_language_hint, setting_show_welcome, setting_show_welcome_hint,
        setting_check_updates, setting_check_updates_hint,
        // Welcome screen
        welcome_window_title, welcome_subtitle, welcome_close_hint,
        welcome_update_badge, welcome_update_hint,
        welcome_recent_heading, welcome_recent_empty, welcome_recent_missing,
        welcome_recent_missing_hint,
        welcome_templates_heading, welcome_new_project, welcome_open_project,
        welcome_open_empty, welcome_import_bundle, welcome_import_flp, welcome_settings,
        welcome_show_on_startup, welcome_continue,
        welcome_tab_templates, welcome_tab_genres, welcome_tab_tide, welcome_tab_forge,
        welcome_search_presets, welcome_no_match,
        // Templates
        tpl_tag_ch,
        tpl_beat_title, tpl_beat_desc, tpl_demo_title, tpl_demo_desc, tpl_tide_title, tpl_tide_desc,
        tpl_forge_title, tpl_forge_desc, tpl_vocal_title, tpl_vocal_desc, tpl_blank_title,
        tpl_blank_desc,
        // Genres
        genre_lofi_title, genre_lofi_blurb, genre_trap_title, genre_trap_blurb,
        genre_house_title, genre_house_blurb, genre_synthwave_title, genre_synthwave_blurb,
        genre_chiptune_title, genre_chiptune_blurb, genre_dnb_title, genre_dnb_blurb,
        genre_ambient_title, genre_ambient_blurb,
        // Update checker
        update_title, update_checking, update_up_to_date, update_available, update_install_now,
        update_downloading, update_verifying, update_installing, update_ready_restart,
        update_restart_now, update_restart_save_hint, update_download_linux, update_open_release,
        update_skip_version, update_check_again, update_close, update_later, update_source_build,
        update_copy_command, update_failed, update_nightly, update_nightly_hint,
        // Channel rack
        rack_add_channel, rack_add_channel_tip, rack_unmute_all, rack_clear_pattern,
        rack_pattern_cleared, rack_clone_selected, rack_move_up, rack_move_down,
        rack_delete_selected, rack_route_free, rack_graph_editor, rack_pattern_length,
        rack_swing, rack_swing_tip, rack_swing_status,
        rack_muted, rack_unmuted, rack_soloed, rack_all_play,
        rack_step_off, rack_step_vel, rack_off_grid_status,
        rack_steps_shifted_left, rack_steps_shifted_right, rack_steps_inverted, rack_steps_cleared,
        rack_copy_notes, rack_cut_notes, rack_paste_replace, rack_dup_right, rack_clear_notes,
        rack_clone_channel, rack_clone_channel_tip, rack_no_notes_in_pattern, rack_copy_notes_first,
        rack_humanize_title, rack_humanize_desc, rack_humanize_vel, rack_humanize_timing,
        rack_humanize_ticks, rack_humanize_seed, rack_humanize_applies, rack_humanize_status,
        rack_beat_label, rack_replaced_instrument, rack_select_tip, rack_step_a11y,
        rack_off_grid_hover, rack_added_as_channel, rack_plays_through, rack_all_inserts_busy,
        rack_graph_vel_label, rack_graph_vel_status,
        rack_ch_on_a11y, rack_mute_solo_tip, rack_pan, rack_pan_tip, rack_vol, rack_vol_tip,
        rack_pan_status, rack_vol_status, rack_mixer_insert_tip,
        rack_ch_plugin_hover, rack_ch_inst_hover,
        rack_menu_inst_win, rack_menu_load_preset, rack_menu_steps, rack_menu_shift_left,
        rack_menu_shift_right, rack_menu_fill_2, rack_menu_fill_4, rack_menu_fill_8,
        rack_menu_invert, rack_menu_clear, rack_menu_humanize, rack_menu_delete_ch,
        rack_slide_to, rack_open_roll_tag,
        // Playlist
        pl_clip_deleted, pl_meter_set, pl_no_markers, pl_no_marker_after, pl_no_marker_before,
        pl_looping_marker, pl_placed_track1, pl_made_unique, pl_clip_muted, pl_clip_unmuted,
        pl_tool_selected, pl_empty, pl_add_clip, pl_add_clip_tip, pl_make_unique,
        pl_make_unique_tip, pl_mute_clip, pl_unmute_clip, pl_delete_clip, pl_play_song_mode,
        pl_follow_tip, pl_track_caps, pl_end_tag, pl_track_n,
        pl_track_mute_tip, pl_track_unmute_tip, pl_track_muted, pl_track_unmuted,
        pl_track_renamed, pl_track_rename_tip, pl_track_default_colour,
        pl_track_colour_reset, pl_track_colour_set, pl_track_mute, pl_track_unmute,
        pl_track_solo, pl_track_unsolo, pl_track_soloed, pl_track_unsoloed,
        pl_track_clear, pl_track_cleared, pl_empty_grid_prompt,
        pl_open_in_rack, pl_open_in_roll, pl_rename_pattern, pl_rename_pattern_tip,
        pl_pattern_colour, pl_delete, pl_clip_tip,
        pl_clip_sliced, pl_placed_at_track, pl_marker_tip, pl_marker_header,
        pl_loop_to_next_marker, pl_remove_marker, pl_marker_removed, pl_add_marker_at,
        pl_marker_added, pl_clear_time_sel, pl_time_sig_from_bar, pl_remove_change_here,
        pl_meter_removed, pl_time_sel_cleared, pl_looping_span,
        pl_clip_moved, pl_clip_length, pl_clip_trimmed, pl_clip_duplicated,
        // Piano roll
        roll_channel_fallback, roll_loop_cleared, roll_looping_span,
        roll_lane_fold_tip, roll_lane_open_tip,
        roll_edit_select_all, roll_edit_deselect, roll_edit_copy, roll_edit_cut,
        roll_edit_paste, roll_edit_duplicate,
        roll_tool_quick_quantize, roll_tool_quantize, roll_tool_quick_legato,
        roll_tool_chop, roll_tool_glue, roll_tool_strum, roll_tool_arpeggiate,
        roll_tool_randomize, roll_tool_flip, roll_tool_limit,
        roll_tool_mute_unmute, roll_tool_porta_toggle, roll_tool_octave_up,
        roll_tool_octave_down, roll_tool_lyrics, roll_tool_lyrics_tip,
        roll_porta_new, roll_porta_new_tip,
        roll_menu_scale, roll_scale_off, roll_scale_root, roll_scale_snap, roll_scale_snap_tip,
        roll_menu_snap, roll_menu_tool, roll_menu_note_color,
        roll_menu_target_ch, roll_menu_target_ctrl,
        roll_ghost_notes, roll_ghost_notes_tip, roll_fit_pattern,
        roll_vel_label, roll_vel_hint, roll_channel_tip,
        roll_batch_strum_desc, roll_batch_arp_desc, roll_batch_rand_desc,
        roll_batch_flip_desc, roll_batch_limit_desc, roll_steps_unit,
        roll_delay_per_note, roll_note_length, roll_direction,
        roll_dir_up, roll_dir_down, roll_dir_up_down,
        roll_start_pm, roll_vel_pm, roll_flip_axis, roll_flip_vertical, roll_flip_horizontal,
        roll_pitch, roll_range_to,
        roll_batch_applies_all, roll_batch_applies_sel, roll_batch_applied, roll_batch_no_notes,
        roll_bar_caps, roll_slide_to, roll_lyric_hint,
        roll_note_placed, roll_notes_copied_to, roll_notes_moved_to, roll_notes_resized,
        roll_erased, roll_notes_muted_status, roll_notes_unmuted_status,
        roll_slide_help, roll_slide_edited, roll_painted, roll_slice_none, roll_slide_added,
        roll_porta_select_first, roll_new_notes_color, roll_nothing_to_paste,
        roll_snap_off_quantize, roll_already_on_grid, roll_ghosts_on, roll_ghosts_off,
        roll_lane_target_tip, roll_lane_set_status,
        roll_pitch_vocal_only, roll_pitch_note_tip, roll_pitch_range_tip,
        roll_pitch_curve_header, roll_pitch_remove_pt, roll_pitch_vib_header,
        roll_pitch_vib_channel, roll_pitch_vib_own, roll_pitch_vib_wide, roll_pitch_vib_none,
        roll_pitch_clear_curve,
        roll_legato_none, roll_snap_off_chop, roll_chop_none_longer, roll_glue_none,
        roll_quantize_title, roll_quantize_all_notes, roll_quantize_sub,
        roll_quantize_strength, roll_quantize_starts, roll_quantize_ends,
        lane_velocity, lane_velocity_short, lane_pan, lane_pan_short,
        lane_release, lane_release_short, lane_fine, lane_fine_short,
        lane_mod_x, lane_mod_x_short, lane_mod_y, lane_mod_y_short,
        lane_pitch, lane_pitch_short,
        scale_major, scale_minor, scale_harmonic_minor, scale_melodic_minor,
        scale_dorian, scale_phrygian, scale_lydian, scale_mixolydian, scale_locrian,
        scale_major_pentatonic, scale_minor_pentatonic, scale_blues,
        curve_smooth, curve_linear, curve_ease_in, curve_ease_out,
        color_red, color_blue, color_green, color_yellow, color_orange, color_white,
        color_pink, color_purple, color_cyan, color_lime, color_gold, color_brown,
        color_rose, color_teal, color_indigo, color_grey,
        // Mixer
        mixer_master, mixer_insert_n, mixer_selected,
        mixer_save_preset, mixer_load_preset, mixer_reset_track,
        mixer_route_channels_here, mixer_solo_selected, mixer_solo_shortcut,
        mixer_unmute_all, mixer_arm, mixer_disarm, mixer_arm_shortcut,
        mixer_add_insert, mixer_all_inserts_play, mixer_insert_soloed,
        mixer_strip_eq_caps, mixer_eq_low, mixer_eq_mid, mixer_eq_high, mixer_eq_band_hint,
        mixer_select_channels_first,
        mixer_insert_before, mixer_insert_after, mixer_delete_track,
        mixer_armed_tip, mixer_arm_tip,
        mixer_muted_tip, mixer_mute_tip, mixer_solo_lamp_tip,
        mixer_muted_solo_tip, mixer_mute_solo_tip,
        mixer_sep, mixer_sep_narrower, mixer_sep_wider, mixer_sep_hint,
        mixer_pan_short_hint, mixer_pan_hint, mixer_vol_hint, mixer_clip_tip,
        mixer_add_send_to, mixer_already_sends, mixer_would_loop_tip, mixer_add_send_100_tip,
        mixer_remove_all_sends, mixer_route_to_master, mixer_master_no_out,
        mixer_no_longer_master, mixer_feeds_master, mixer_cant_feed_self, mixer_would_loop,
        mixer_routes_only, mixer_already_master, mixer_already_sends_pct, mixer_now_sends,
        mixer_no_sends, mixer_all_sends_removed,
        mixer_to_prefix, mixer_to_nowhere, mixer_ducks_under, mixer_sidechains,
        mixer_effects_caps, mixer_slot_on_tip, mixer_slot_bypass_tip,
        mixer_slot_mix, mixer_slot_mix_hint, mixer_slot_empty, mixer_slot_hover,
        mixer_ext_plugins, mixer_checking_plugins, mixer_no_fx_plugins, mixer_plugin_settings,
        mixer_save_fx_preset, mixer_load_fx_preset,
        mixer_sidechain_input, mixer_sidechain_plugin_hint, mixer_remove,
        mixer_effect_removed, mixer_slot_set_status, mixer_loading_plugin,
        mixer_ir_not_loaded, mixer_dlg_wave, mixer_ir_unreadable, mixer_ir_loaded,
        mixer_fx_on, mixer_fx_bypassed, mixer_fx_reset, mixer_fx_reset_tip,
        mixer_fx_ab_tip, mixer_fx_presets, mixer_fx_presets_tip, mixer_fx_ab_hint,
        mixer_ir_studio_builtin, mixer_ir_label, mixer_ir_builtin_tip,
        mixer_ir_load_btn, mixer_ir_builtin_btn,
        mixer_sc_caps, mixer_sc_off_own, mixer_sc_tip, mixer_sc_loop_tip,
        mixer_sc_ducks_status, mixer_sc_plugin_status, mixer_sc_own_status, mixer_sc_none_status,
        mixer_peq_freq, mixer_peq_gain, mixer_peq_q,
        mixer_hide_editor, mixer_open_plugin,
        mixer_sends_caps, mixer_send_label, mixer_send_hint,
        mixer_remove_send, mixer_add_send, mixer_add_send_tip,
        fx_cat_delay_reverb, fx_cat_distortion, fx_cat_dynamics, fx_cat_eq_filters,
        fx_cat_modulation, fx_cat_plugin, fx_cat_stereo, fx_cat_time, fx_cat_vocal,
        fx_grp_dynamics, fx_grp_timing_output, fx_grp_level, fx_grp_release,
        fx_grp_echo, fx_grp_tone_mix, fx_grp_space, fx_grp_filter_mix,
        fx_grp_squarize, fx_grp_puncher, fx_grp_filter, fx_grp_output,
        fx_grp_delay_line, fx_grp_movement, fx_grp_filters, fx_grp_left_right,
        fx_grp_delay_phase, fx_grp_controls,
        // Browser
        browser_tags, browser_tags_with, browser_add_tags_hint, browser_remove_all_tags,
        browser_search_assets, browser_instruments, browser_no_instruments,
        browser_click_adds_rack, browser_add_to_rack_full, browser_add_to_rack,
        browser_remove_fav, browser_add_fav,
        browser_effects, browser_slots_full, browser_add_to_mixer,
        browser_no_matching_fx, browser_click_puts_on,
        browser_presets, browser_packs, browser_libraries, browser_recorded,
        browser_rendered, browser_folders, browser_files_path,
        browser_preset_hover, browser_load, browser_open_folder, browser_sample_hover,
        browser_tide_load_tip, browser_tide_new_tip, browser_load_into, browser_add_new_channel,
        browser_saved, browser_favorites, browser_match, browser_match_any, browser_match_all,
        browser_pick_tags_hint, browser_no_tags_match, browser_asset_missing,
        browser_load_selected_ch, browser_add_plugin_title, browser_search_plugins,
        browser_no_plugins_found,
        preset_folder_channel, preset_folder_mixer, preset_folder_fx, preset_folder_synth,
        synth_cat_arp, synth_cat_bass, synth_cat_bell, synth_cat_chiptune, synth_cat_drum,
        synth_cat_fx, synth_cat_init, synth_cat_keys, synth_cat_lead, synth_cat_pad,
        synth_cat_pluck,
    },
    plurals: {
        clips_deleted,
        deck_rec_tracks_tip,
        deck_rec_auto_tip,
        pattern_deleted_clips,
        bundled_irs,
        bundled_plugin_refs,
        bundled_missing_irs,
        bundle_restored_irs,
        flp_missing_plugins,
        flp_plugins_count,
        flp_samples_restored,
        rack_bars,
        rack_steps,
        rack_filled_steps,
        rack_notes_cut,
        rack_notes_copied,
        rack_notes_pasted,
        rack_notes_repeated,
        rack_notes_removed,
        rack_channels_cloned,
        rack_channels_deleted,
        pl_clips_count,
        pl_shares_notes,
        pl_clips_selected,
        pl_clips_moved,
        pl_clips_duplicated,
        roll_ticks,
        roll_notes_count,
        roll_notes_selected,
        roll_notes_cut_at,
        roll_porta_on_notes,
        roll_porta_off_notes,
        roll_notes_coloured,
        roll_notes_deleted,
        roll_notes_copied,
        roll_notes_pasted,
        roll_notes_duplicated,
        roll_notes_quantized,
        roll_notes_muted_n,
        roll_notes_unmuted_n,
        roll_legato_n,
        roll_chopped_n,
        roll_glued_n,
        roll_quantize_sel_n,
        roll_quantize_pct_n,
        mixer_inserts_count,
        mixer_channels_routed,
        browser_plugin_problems,
    },
}

impl Strings {
    pub fn clips_deleted_msg(&self, n: usize) -> String {
        self.clips_deleted.fmt(self.lang, n)
    }

    pub fn deck_monitor_label(&self, mode: crate::settings::DeckMonitor) -> &'static str {
        match mode {
            crate::settings::DeckMonitor::Off => self.monitor_off,
            crate::settings::DeckMonitor::Scope => self.monitor_scope,
            crate::settings::DeckMonitor::Spectrum => self.monitor_spectrum,
        }
    }

    pub fn theme_mode_label(&self, mode: crate::settings::ThemeMode) -> &'static str {
        match mode {
            crate::settings::ThemeMode::Dark => self.theme_dark,
            crate::settings::ThemeMode::Light => self.theme_light,
            crate::settings::ThemeMode::System => self.theme_system,
        }
    }

    pub fn workspace_preset(&self, i: usize) -> &'static str {
        match i {
            0 => self.ws_arrange,
            1 => self.ws_compose,
            _ => self.ws_edit,
        }
    }

    pub fn kind_title(&self, kind: crate::ui::Kind) -> &'static str {
        match kind {
            crate::ui::Kind::Playlist => self.win_playlist,
            crate::ui::Kind::Rack => self.win_rack,
            crate::ui::Kind::Roll => self.win_roll,
            crate::ui::Kind::Instrument => self.win_instrument,
            crate::ui::Kind::Mixer => self.win_mixer,
            crate::ui::Kind::Riff => self.win_riff,
            crate::ui::Kind::Settings => self.settings_title,
        }
    }

    pub fn tool_label(&self, tool: crate::ui::Tool) -> &'static str {
        match tool {
            crate::ui::Tool::Draw => self.tool_draw,
            crate::ui::Tool::Paint => self.tool_paint,
            crate::ui::Tool::Delete => self.tool_delete,
            crate::ui::Tool::Mute => self.tool_mute,
            crate::ui::Tool::Slice => self.tool_slice,
            crate::ui::Tool::Select => self.tool_select,
            crate::ui::Tool::Zoom => self.tool_zoom,
            crate::ui::Tool::Slide => self.tool_slide,
        }
    }

    pub fn tool_hint(&self, tool: crate::ui::Tool) -> &'static str {
        match tool {
            crate::ui::Tool::Draw => self.tool_draw_hint,
            crate::ui::Tool::Paint => self.tool_paint_hint,
            crate::ui::Tool::Delete => self.tool_delete_hint,
            crate::ui::Tool::Mute => self.tool_mute_hint,
            crate::ui::Tool::Slice => self.tool_slice_hint,
            crate::ui::Tool::Select => self.tool_select_hint,
            crate::ui::Tool::Zoom => self.tool_zoom_hint,
            crate::ui::Tool::Slide => self.tool_slide_hint,
        }
    }

    pub fn snap_label(&self, snap: crate::ui::Snap) -> &'static str {
        match snap {
            crate::ui::Snap::Half => "1/32",
            crate::ui::Snap::Step => "1/16",
            crate::ui::Snap::Triplet => "1/8T",
            crate::ui::Snap::Beat => "1/4",
            crate::ui::Snap::Bar => self.snap_bar,
            crate::ui::Snap::Off => self.snap_off,
        }
    }

    pub fn template_title(&self, kind: crate::ui::welcome::TemplateKind) -> &'static str {
        use crate::ui::welcome::TemplateKind as T;
        match kind {
            T::EmptyBeat => self.tpl_beat_title,
            T::DemoBeat => self.tpl_demo_title,
            T::TideStudio => self.tpl_tide_title,
            T::ForgeSfx => self.tpl_forge_title,
            T::VocalSession => self.tpl_vocal_title,
            T::BlankMinimal => self.tpl_blank_title,
        }
    }

    pub fn template_desc(&self, kind: crate::ui::welcome::TemplateKind) -> &'static str {
        use crate::ui::welcome::TemplateKind as T;
        match kind {
            T::EmptyBeat => self.tpl_beat_desc,
            T::DemoBeat => self.tpl_demo_desc,
            T::TideStudio => self.tpl_tide_desc,
            T::ForgeSfx => self.tpl_forge_desc,
            T::VocalSession => self.tpl_vocal_desc,
            T::BlankMinimal => self.tpl_blank_desc,
        }
    }

    pub fn genre_title(&self, genre: keyframexl::genres::Genre) -> &'static str {
        use keyframexl::genres::Genre as G;
        match genre {
            G::LoFi => self.genre_lofi_title,
            G::Trap => self.genre_trap_title,
            G::House => self.genre_house_title,
            G::Synthwave => self.genre_synthwave_title,
            G::Chiptune => self.genre_chiptune_title,
            G::DrumAndBass => self.genre_dnb_title,
            G::Ambient => self.genre_ambient_title,
        }
    }

    pub fn genre_blurb(&self, genre: keyframexl::genres::Genre) -> &'static str {
        use keyframexl::genres::Genre as G;
        match genre {
            G::LoFi => self.genre_lofi_blurb,
            G::Trap => self.genre_trap_blurb,
            G::House => self.genre_house_blurb,
            G::Synthwave => self.genre_synthwave_blurb,
            G::Chiptune => self.genre_chiptune_blurb,
            G::DrumAndBass => self.genre_dnb_blurb,
            G::Ambient => self.genre_ambient_blurb,
        }
    }

    pub fn length_text(&self, t: keyframexl::sequence::Tick) -> String {
        use keyframexl::sequence::STEP;
        match (t / STEP, t % STEP) {
            (s, 0) => self.rack_steps.fmt(self.lang, s.unsigned_abs() as usize),
            (0, r) => self.roll_ticks.fmt(self.lang, r.unsigned_abs() as usize),
            (s, r) => format!(
                "{} +{}",
                self.rack_steps.fmt(self.lang, s.unsigned_abs() as usize),
                self.roll_ticks.fmt(self.lang, r.unsigned_abs() as usize)
            ),
        }
    }

    pub fn lane_target_label(&self, target: crate::ui::roll::LaneTarget) -> &'static str {
        use crate::ui::roll::LaneTarget as L;
        match target {
            L::Velocity => self.lane_velocity,
            L::Pan => self.lane_pan,
            L::Release => self.lane_release,
            L::Fine => self.lane_fine,
            L::ModX => self.lane_mod_x,
            L::ModY => self.lane_mod_y,
            L::Pitch => self.lane_pitch,
        }
    }

    pub fn lane_target_short(&self, target: crate::ui::roll::LaneTarget) -> &'static str {
        use crate::ui::roll::LaneTarget as L;
        match target {
            L::Velocity => self.lane_velocity_short,
            L::Pan => self.lane_pan_short,
            L::Release => self.lane_release_short,
            L::Fine => self.lane_fine_short,
            L::ModX => self.lane_mod_x_short,
            L::ModY => self.lane_mod_y_short,
            L::Pitch => self.lane_pitch_short,
        }
    }

    pub fn scale_kind_label(&self, kind: keyframexl::scale::Kind) -> &'static str {
        use keyframexl::scale::Kind as K;
        match kind {
            K::Major => self.scale_major,
            K::Minor => self.scale_minor,
            K::HarmonicMinor => self.scale_harmonic_minor,
            K::MelodicMinor => self.scale_melodic_minor,
            K::Dorian => self.scale_dorian,
            K::Phrygian => self.scale_phrygian,
            K::Lydian => self.scale_lydian,
            K::Mixolydian => self.scale_mixolydian,
            K::Locrian => self.scale_locrian,
            K::MajorPentatonic => self.scale_major_pentatonic,
            K::MinorPentatonic => self.scale_minor_pentatonic,
            K::Blues => self.scale_blues,
        }
    }

    pub fn curve_shape_label(&self, shape: keyframexl::sequence::CurveShape) -> &'static str {
        use keyframexl::sequence::CurveShape as C;
        match shape {
            C::Smooth => self.curve_smooth,
            C::Linear => self.curve_linear,
            C::EaseIn => self.curve_ease_in,
            C::EaseOut => self.curve_ease_out,
        }
    }

    pub fn note_color_name(&self, idx: u8) -> &'static str {
        match idx as usize % 16 {
            0 => self.color_red,
            1 => self.color_blue,
            2 => self.color_green,
            3 => self.color_yellow,
            4 => self.color_orange,
            5 => self.color_white,
            6 => self.color_pink,
            7 => self.color_purple,
            8 => self.color_cyan,
            9 => self.color_lime,
            10 => self.color_gold,
            11 => self.color_brown,
            12 => self.color_rose,
            13 => self.color_teal,
            14 => self.color_indigo,
            _ => self.color_grey,
        }
    }

    pub fn fx_category_label(&self, category: &'static str) -> &'static str {
        match category {
            "Delay & reverb" => self.fx_cat_delay_reverb,
            "Distortion" => self.fx_cat_distortion,
            "Dynamics" => self.fx_cat_dynamics,
            "EQ & filters" => self.fx_cat_eq_filters,
            "Modulation" => self.fx_cat_modulation,
            "Plugin" => self.fx_cat_plugin,
            "Stereo" => self.fx_cat_stereo,
            "Time" => self.fx_cat_time,
            "Vocal" => self.fx_cat_vocal,
            other => other,
        }
    }

    pub fn fx_param_group_label(&self, kind: keyframexl::fx::FxKind, group: usize) -> &'static str {
        use keyframexl::fx::FxKind as F;
        let list: &[&'static str] = match kind {
            F::Compressor => &[self.fx_grp_dynamics, self.fx_grp_timing_output],
            F::Limiter => &[self.fx_grp_level, self.fx_grp_release],
            F::Delay => &[self.fx_grp_echo, self.fx_grp_tone_mix],
            F::Reverb => &[self.fx_grp_space, self.fx_grp_filter_mix],
            F::Squeeze => &[
                self.fx_grp_squarize,
                self.fx_grp_puncher,
                self.fx_grp_filter,
                self.fx_grp_output,
            ],
            F::HyperChorus => &[
                self.fx_grp_delay_line,
                self.fx_grp_movement,
                self.fx_grp_filters,
                self.fx_grp_output,
            ],
            F::StereoShaper => &[self.fx_grp_left_right, self.fx_grp_delay_phase],
            _ => &[self.fx_grp_controls, self.fx_grp_output],
        };
        list.get(group).copied().unwrap_or(list[list.len() - 1])
    }

    pub fn preset_kind_folder_label(&self, kind: keyframexl::preset::Kind) -> &'static str {
        use keyframexl::preset::Kind as K;
        match kind {
            K::Channel => self.preset_folder_channel,
            K::Mixer => self.preset_folder_mixer,
            K::Fx => self.preset_folder_fx,
            K::Synth => self.preset_folder_synth,
        }
    }

    pub fn synth_category_label(&self, category: &'static str) -> &'static str {
        match category {
            "Arp" => self.synth_cat_arp,
            "Bass" => self.synth_cat_bass,
            "Bell" => self.synth_cat_bell,
            "Chiptune" => self.synth_cat_chiptune,
            "Drum" => self.synth_cat_drum,
            "FX" => self.synth_cat_fx,
            "Init" => self.synth_cat_init,
            "Keys" => self.synth_cat_keys,
            "Lead" => self.synth_cat_lead,
            "Pad" => self.synth_cat_pad,
            "Pluck" => self.synth_cat_pluck,
            other => other,
        }
    }
}

pub const fn strings(lang: Language) -> &'static Strings {
    match lang {
        Language::English => &EN,
        Language::Ukrainian => &UK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_is_non_empty() {
        for (lang, table) in [("EN", &EN), ("UK", &UK)] {
            for (name, value) in table.entries() {
                assert!(!value.trim().is_empty(), "{lang}.{name} is empty");
            }
            for (name, p) in table.plural_entries() {
                for (form, value) in [
                    ("one", p.one),
                    ("few", p.few),
                    ("many", p.many),
                    ("other", p.other),
                ] {
                    assert!(!value.trim().is_empty(), "{lang}.{name}.{form} is empty");
                }
            }
        }
    }

    #[test]
    fn both_tables_have_the_same_fields() {
        assert_eq!(EN.entries().len(), UK.entries().len());
        let en: Vec<_> = EN.entries().iter().map(|(n, _)| *n).collect();
        let uk: Vec<_> = UK.entries().iter().map(|(n, _)| *n).collect();
        assert_eq!(en, uk);
        let en_p: Vec<_> = EN.plural_entries().iter().map(|(n, _)| *n).collect();
        let uk_p: Vec<_> = UK.plural_entries().iter().map(|(n, _)| *n).collect();
        assert_eq!(en_p, uk_p);
    }

    #[test]
    fn language_labels_are_distinct() {
        let labels: Vec<_> = Language::ALL.iter().map(|l| l.label()).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        for (i, a) in labels.iter().enumerate() {
            for b in &labels[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn strings_selects_the_matching_table() {
        assert_eq!(strings(Language::English).lang, Language::English);
        assert_eq!(strings(Language::Ukrainian).lang, Language::Ukrainian);
        assert_eq!(strings(Language::English).menu_file, EN.menu_file);
        assert_eq!(strings(Language::Ukrainian).menu_file, UK.menu_file);
        assert_eq!(Language::default(), Language::English);
    }

    #[test]
    fn locale_detection_handles_ukrainian_and_fallbacks() {
        assert_eq!(Language::from_locale("uk_UA.UTF-8"), Language::Ukrainian);
        assert_eq!(Language::from_locale("uk-UA"), Language::Ukrainian);
        assert_eq!(Language::from_locale("uk"), Language::Ukrainian);
        assert_eq!(Language::from_locale("UK_UA:en_US"), Language::Ukrainian);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("en_US:uk_UA"), Language::English);
        assert_eq!(Language::from_locale("de_DE.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("C"), Language::English);
        assert_eq!(Language::from_locale(""), Language::English);
    }

    #[test]
    fn ukrainian_plural_categories() {
        use Plural::*;
        let cases = [
            (0, Many),
            (1, One),
            (2, Few),
            (3, Few),
            (4, Few),
            (5, Many),
            (10, Many),
            (11, Many),
            (12, Many),
            (14, Many),
            (20, Many),
            (21, One),
            (22, Few),
            (24, Few),
            (25, Many),
            (100, Many),
            (101, One),
            (111, Many),
            (112, Many),
            (121, One),
            (122, Few),
        ];
        for (n, want) in cases {
            assert_eq!(Language::Ukrainian.plural(n), want, "n = {n}");
        }
        assert_eq!(Language::Ukrainian.plural_f64(1.5), Other);
        assert_eq!(Language::Ukrainian.plural_f64(2.0), Few);
        assert_eq!(Language::English.plural(1), One);
        assert_eq!(Language::English.plural(0), Other);
        assert_eq!(Language::English.plural(2), Other);
        assert_eq!(Language::English.plural_f64(1.5), Other);
    }

    #[test]
    fn ukrainian_counts_read_naturally() {
        assert_eq!(
            UK.clips_deleted_msg(0),
            "0 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(UK.clips_deleted_msg(1), "1 кліп видалено. Ctrl+Z скасовує.");
        assert_eq!(
            UK.clips_deleted_msg(2),
            "2 кліпи видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(5),
            "5 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(11),
            "11 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(21),
            "21 кліп видалено. Ctrl+Z скасовує."
        );
        assert_eq!(EN.clips_deleted_msg(1), "1 clip deleted. Ctrl+Z undoes.");
        assert_eq!(EN.clips_deleted_msg(2), "2 clips deleted. Ctrl+Z undoes.");
    }

    #[test]
    fn fill_substitutes_named_placeholders_without_rescanning() {
        assert_eq!(
            fill("Hello {name}, {n} items", &[("name", "Tide"), ("n", "3")]),
            "Hello Tide, 3 items"
        );
        assert_eq!(
            fill("{a} {b}", &[("a", "{b}"), ("b", "ok")]),
            "{b} ok",
            "substituted values are not re-scanned"
        );
        assert_eq!(fill("keep {unknown}", &[("x", "1")]), "keep {unknown}");
        assert_eq!(
            fill("unclosed {brace", &[("brace", "x")]),
            "unclosed {brace"
        );
    }

    #[test]
    fn ukrainian_translates_key_labels() {
        let pairs = [
            ("menu_file", EN.menu_file, UK.menu_file),
            ("file_new", EN.file_new, UK.file_new),
            ("help_welcome", EN.help_welcome, UK.help_welcome),
            ("deck_pattern", EN.deck_pattern, UK.deck_pattern),
            ("deck_mixer", EN.deck_mixer, UK.deck_mixer),
            ("settings_title", EN.settings_title, UK.settings_title),
            (
                "welcome_new_project",
                EN.welcome_new_project,
                UK.welcome_new_project,
            ),
            (
                "welcome_templates_heading",
                EN.welcome_templates_heading,
                UK.welcome_templates_heading,
            ),
            ("update_title", EN.update_title, UK.update_title),
            ("tpl_blank_title", EN.tpl_blank_title, UK.tpl_blank_title),
        ];
        for (name, en, uk) in pairs {
            assert_ne!(en, uk, "{name} is not translated");
        }
    }
}
