//! UI localization: English and Ukrainian string tables, CLDR plurals,
//! placeholder formatting, and system locale detection.

#![allow(dead_code)]

mod en;
mod plural;
#[cfg(test)]
mod scan_tests;
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
        status_tempo_from_taps,
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
        settings_title, setting_language,
        setting_language_hint, setting_show_welcome, setting_show_welcome_hint,
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
        update_skip_version, update_check_again, update_later, update_source_build,
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
        // Shared kit widgets
        kit_create_auto, kit_create_auto_tip, kit_link_ctrl, kit_link_ctrl_tip,
        kit_reset, kit_reset_back_to, kit_type_value, kit_copy_value, kit_paste_value,
        kit_auto_unavailable, kit_link_unavailable, kit_knob_tip_footer,
        // Instrument window & menus
        inst_sampler_added, inst_drum_added,
        inst_menu_sampler, inst_menu_sampler_tip, inst_menu_tide,
        inst_menu_forge, inst_menu_forge_tip, inst_menu_drum_kit, inst_menu_drum_kit_tip,
        inst_kit_core, inst_kit_chip, inst_menu_vocal, inst_menu_vocal_tip,
        inst_menu_more_plugins, inst_menu_drum_voice, inst_no_plugins, inst_deleted,
        inst_dlg_sampler_filter, inst_loading_into, inst_drop_unsupported, inst_drop_new_sampler,
        inst_gone, inst_sub_ch, inst_load_sound_menu, inst_save_ch_preset, inst_load_ch_preset,
        inst_delete_channel, inst_loading_btn, inst_replace_sound, inst_replace_sound_tip,
        inst_roll_btn_tip, inst_ch_name_hint, inst_click_rename, inst_drum_desc,
        inst_loading_sound, inst_empty_sampler_head, inst_empty_sampler_desc,
        inst_load_sound_a11y, inst_load_sound_btn,
        // Sampler editor
        smp_page_program, smp_page_zone, smp_page_filter, smp_page_mod, smp_page_zones,
        smp_loop_off, smp_loop_fwd, smp_loop_oneshot, smp_loop_sustained, smp_loop_bounce,
        smp_bipolar_tip, smp_val_out_of_range, smp_no_zones, smp_editing_zone,
        smp_zn_short, smp_scope_all, smp_scope_zone, smp_audition_tip, smp_zone_map_tip,
        smp_sec_prog_out, smp_k_volume, smp_k_volume_hint, smp_k_pan, smp_k_pan_hint,
        smp_k_trans, smp_k_trans_hint, smp_k_pb_rng, smp_k_pb_rng_hint,
        smp_sec_voice_glide, smp_k_poly, smp_k_poly_hint, smp_k_glide, smp_k_glide_hint,
        smp_off_lower, smp_sec_summary, smp_summary_line,
        smp_sec_pitch_root, smp_root_up_tip, smp_root_down_tip, smp_root_badge,
        smp_k_semi, smp_k_semi_hint, smp_k_fine, smp_k_fine_hint, smp_k_ktrk, smp_k_ktrk_hint,
        smp_sec_amp_grp, smp_k_gain, smp_k_gain_hint, smp_k_zone_pan_hint,
        smp_k_vel, smp_k_vel_hint, smp_k_group, smp_k_group_hint,
        smp_sec_stretch, smp_k_length, smp_k_length_hint, smp_k_beats, smp_k_beats_hint,
        smp_stretch_recorded, smp_stretch_rendering, smp_stretch_ready, smp_stretch_arrow,
        smp_sec_amp_env,
        smp_dir_fwd, smp_dir_rev, smp_snap_0x, smp_snap_0x_tip, smp_fit, smp_fit_tip,
        smp_zoom_in_tip, smp_zoom_out_tip, smp_wave_len_loop, smp_wave_len_frames, smp_wave_tip,
        smp_sec_dual_filter, smp_route_serial, smp_route_parallel, smp_filter_n,
        smp_fk_short_off, smp_fk_short_notch, smp_k_freq, smp_k_freq_hint,
        smp_k_emph, smp_k_emph_hint, smp_env_unrouted, smp_env_routed,
        smp_filters_bypassed, smp_dual_filter_tip,
        smp_lfo_sync, smp_lfo_free, smp_k_rate, smp_k_lfo_rate_hint,
        smp_k_attack, smp_k_mod_fade_hint, smp_k_phase, smp_k_phase_hint,
        smp_mod_matrix_caps, smp_mod_depth_hint, smp_clear_slot_tip,
        smp_col_wave, smp_col_sample, smp_col_keys, smp_col_root, smp_col_vel, smp_col_loop,
        smp_env_curve_tip, smp_k_delay, smp_k_delay_hint, smp_k_attack_hint,
        smp_k_hold, smp_k_hold_hint, smp_k_decay, smp_k_decay_hint,
        smp_k_sustain, smp_k_sustain_hint, smp_k_release, smp_k_release_hint,
        smp_play_poly, smp_play_mono, smp_play_legato,
        smp_glide_fixed, smp_glide_auto,
        smp_stretch_off, smp_stretch_time, smp_stretch_beat,
        smp_fk_off, smp_fk_lp, smp_fk_hp, smp_fk_bp, smp_fk_notch, smp_fk_ap,
        smp_fr_serial, smp_fr_parallel,
        smp_lfo_sine, smp_lfo_abs_sine, smp_lfo_tri, smp_lfo_sq, smp_lfo_saw,
        smp_lfo_inv_saw, smp_lfo_rand, smp_lfo_lp_rand,
        smp_ms_env1, smp_ms_env2, smp_ms_vel, smp_ms_key, smp_ms_rand,
        smp_ms_plfo1, smp_ms_plfo2,
        smp_mt_pitch, smp_mt_gain, smp_mt_pan, smp_mt_cut1, smp_mt_emph1,
        smp_mt_cut2, smp_mt_emph2, smp_mt_lfo1, smp_mt_lfo2,
        // Vocal editor & lyrics
        vocal_installed, vocal_sec_bank, vocal_no_bank, vocal_none_installed,
        vocal_folder_btn, vocal_folder_tip, vocal_install_zip_btn, vocal_install_zip_tip,
        vocal_dlg_utau_filter, vocal_bank_fallback, vocal_installing, vocal_working,
        vocal_download_teto, vocal_download_teto_tip, vocal_downloading_teto, vocal_teto_terms,
        vocal_empty_hint, vocal_rendering_progress, vocal_sec_voice,
        vocal_k_vibrato, vocal_k_vibrato_hint, vocal_k_rate, vocal_k_rate_hint,
        vocal_k_delay, vocal_k_delay_hint, vocal_k_fade_in, vocal_k_fade_in_hint,
        vocal_k_glide, vocal_k_glide_hint, vocal_k_gender, vocal_k_gender_hint,
        vocal_k_breath, vocal_k_breath_hint, vocal_sec_default_lyric, vocal_default_lyric_desc,
        vocal_sec_singing, vocal_singing_p1, vocal_singing_p2, vocal_singing_p3,
        vocal_lyrics_title, vocal_lyrics_desc, vocal_lyrics_summary,
        // KeyFrame Tide synth editor
        tide_tbl_sine_saw, tide_tbl_square, tide_tbl_organ, tide_tbl_vowel,
        tide_wrp_bend, tide_wrp_sync, tide_wrp_mirror, tide_wrp_fm_sub,
        tide_p_position, tide_p_warp, tide_p_cutoff, tide_p_resonance,
        tide_p_sub, tide_p_noise, tide_p_detune, tide_p_spread,
        tide_ps_position, tide_ps_warp, tide_ps_cutoff, tide_ps_resonance,
        tide_ps_sub, tide_ps_noise, tide_ps_detune, tide_ps_spread,
        tide_ms_filter_env, tide_ms_velocity, tide_ms_macro_1, tide_ms_macro_2,
        tide_ms_macro_3, tide_ms_macro_4, tide_ms_invalid_macro, tide_ms_mod_wheel,
        tide_ms_pressure, tide_ms_slide,
        tide_mt_pos_b, tide_mt_warp_b, tide_mt_level_b, tide_mt_pitch_a,
        tide_mt_pitch_b, tide_mt_level_c, tide_mt_tilt_c, tide_mt_odd_c,
        tide_bipolar_tip, tide_hud_pos, tide_hud_muted, tide_hud_xy_wave, tide_wave_scope_tip,
        tide_hud_spectral, tide_hud_xy_spec, tide_spec_scope_tip,
        tide_hud_reso, tide_hud_env, tide_flt_scope_tip,
        tide_env_view_tip, tide_k_attack, tide_k_attack_hint, tide_k_decay, tide_k_decay_hint,
        tide_k_sustain, tide_k_sustain_hint, tide_k_release, tide_k_release_hint,
        tide_lfo_off, tide_lfo_sync_prefix, tide_osc_off, tide_osc_toggle_tip,
        tide_kb_oct_down, tide_kb_oct_up, tide_kb_tip,
        tide_jump_osc_a, tide_jump_osc_b, tide_jump_osc_c, tide_open_fx_bay,
        tide_prev_preset, tide_next_preset, tide_preset_edited, tide_preset_saved_group,
        tide_ab_tip, tide_mutate, tide_mutate_tip, tide_init, tide_init_tip,
        tide_save_btn, tide_save_tip, tide_load_btn, tide_load_tip,
        tide_k_pos, tide_k_pos_a_hint, tide_k_warp, tide_k_warp_a_hint,
        tide_k_level, tide_k_level_a_hint, tide_k_coarse, tide_k_coarse_hint,
        tide_k_fine, tide_k_fine_hint, tide_k_sub, tide_k_sub_hint,
        tide_k_noise, tide_k_noise_hint,
        tide_k_pos_b_hint, tide_k_warp_b_hint, tide_k_level_b_hint,
        tide_k_level_c_hint, tide_k_partials, tide_k_partials_hint,
        tide_k_tilt, tide_k_tilt_hint, tide_k_odd, tide_k_odd_hint,
        tide_k_shimmer, tide_k_shimmer_hint,
        tide_sec_filter, tide_k_cutoff, tide_k_cutoff_hint, tide_k_reso, tide_k_reso_hint,
        tide_k_key, tide_k_key_hint, tide_k_env, tide_k_env_hint,
        tide_unison_lbl, tide_unison_less, tide_unison_more, tide_voice_strip,
        tide_off_lower, tide_edit_btn, tide_edit_voices_tip,
        tide_tab_envelopes, tide_tab_lfo_mod, tide_tab_lfo_mod_n,
        tide_tab_effects, tide_tab_effects_n, tide_tab_voices_macros,
        tide_sec_amp_env, tide_sec_flt_env,
        tide_retrig, tide_retrig_tip, tide_sync, tide_sync_tip,
        tide_shp_off, tide_shp_sine, tide_shp_tri, tide_shp_saw, tide_shp_sq, tide_shp_sh,
        tide_k_rate, tide_k_rate_hint,
        tide_sec_mod_matrix, tide_add_route, tide_mod_empty,
        tide_qr_lfo1_cut, tide_qr_lfo1_pos, tide_qr_vel_cut, tide_qr_wheel_warp,
        tide_mod_depth_hint, tide_remove_route,
        tide_fx_toggle_tip,
        tide_fx_drive, tide_fx_drive_hint, tide_fx_tone, tide_fx_tone_hint,
        tide_fx_mix, tide_fx_mix_hint, tide_fx_depth, tide_fx_depth_hint,
        tide_fx_rate, tide_fx_rate_hint, tide_fx_time, tide_fx_time_hint,
        tide_fx_repeat, tide_fx_repeat_hint, tide_fx_wet, tide_fx_wet_hint,
        tide_fx_size, tide_fx_size_hint, tide_fx_damp, tide_fx_damp_hint,
        tide_fx_reverb_wet_hint,
        tide_sec_voices_mpe, tide_mpe_tip,
        tide_k_voices, tide_k_voices_hint, tide_k_detune, tide_k_detune_hint,
        tide_k_spread, tide_k_spread_hint, tide_k_glide, tide_k_glide_hint,
        tide_k_bend, tide_k_bend_hint,
        tide_sec_macro_routing, tide_macro_heading, tide_macro_depth_hint,
        tide_add_target, tide_macro_knob_n, tide_macro_knob_hint,
        // Forge editor
        forge_added, forge_play, forge_play_caps, forge_play_tip,
        forge_start_from, forge_my_sounds, forge_save_my_sounds, forge_save_my_sounds_tip,
        forge_game_style_caps, forge_style_any, forge_style_retro, forge_style_cartoon,
        forge_style_heavy, forge_style_scifi, forge_game_style_tip,
        forge_make_it_caps, forge_style_retro_tip, forge_style_cartoon_tip,
        forge_style_heavy_tip, forge_style_scifi_tip,
        forge_sec_sound, forge_peak_hud, forge_rendering_caps, forge_click_to_hear,
        forge_sec_character, forge_macro_size, forge_macro_energy, forge_macro_dirt, forge_macro_space,
        forge_macro_size_hint, forge_macro_energy_hint, forge_macro_dirt_hint, forge_macro_space_hint,
        forge_k_pitch, forge_k_pitch_hint, forge_k_output, forge_k_output_hint,
        forge_sec_ideas, forge_growing, forge_mutate, forge_mutate_tip, forge_clear,
        forge_ideas_empty, forge_idea_tile_tip, forge_use_idea, forge_use_idea_tip,
        forge_sec_layer, forge_sec_master_fx, forge_sec_export,
        forge_k_files, forge_k_files_hint, forge_normalize, forge_normalize_hint,
        forge_mono, forge_mono_hint, forge_export_btn, forge_export_tip,
        forge_style_any_msg, forge_style_set_msg,
        forge_render_stopped, forge_bounce_err, forge_bounce_silence,
        forge_layers_full_bounce, forge_bounced_ok,
        forge_saved_ok, forge_save_err, forge_export_dlg_title, forge_export_err,
        forge_sample_filter, forge_sample_dlg_title, forge_sample_read_err, forge_sample_loaded,
        forge_deleted_channel,
        forge_sec_layers, forge_solo_tip, forge_edit_layer_tip, forge_tide_synth_lbl,
        forge_delete_layer_tip, forge_bouncing_new, forge_bouncing_layer,
        forge_layers_empty, forge_add_layer, forge_src_osc, forge_src_noise,
        forge_src_sample_menu, forge_src_sample, forge_src_synth, forge_src_channel_note,
        forge_src_channel_note_tip, forge_src_tide_note,
        forge_layers_max_tip, forge_add_layer_tip, forge_duplicate, forge_duplicate_tip,
        forge_layer_copy_suffix, forge_layer_name_hint,
        forge_k_osc_pitch_hint, forge_k_vibrato, forge_k_vibrato_hint,
        forge_k_rate, forge_k_vib_rate_hint, forge_k_digital_rate_hint, forge_k_crackle_rate_hint,
        forge_k_note, forge_k_bounce_note_hint, forge_k_hold, forge_k_hold_hint,
        forge_bounce_btn, forge_bounce_btn_tip, forge_replace_btn, forge_reverse_lbl,
        forge_edited_patch, forge_k_synth_note_hint,
        forge_k_start, forge_k_start_hint, forge_k_length, forge_k_length_hint,
        forge_k_gain, forge_k_gain_hint, forge_k_pan, forge_k_pan_hint,
        forge_line_pitch, forge_line_volume, forge_line_filter,
        forge_k_reso, forge_k_reso_hint, forge_layer_fx_caps,
        forge_curve_pitch_tip, forge_curve_tip, forge_remove_fx_tip, forge_add_fx,
        forge_cat_ui, forge_cat_steps, forge_cat_actions, forge_cat_weapons,
        forge_cat_pickups, forge_cat_magic, forge_cat_damage, forge_cat_doors,
        forge_cat_ambience, forge_cat_creatures,
        forge_wave_square, forge_wave_pulse25, forge_wave_pulse12,
        forge_wave_tri, forge_wave_saw, forge_wave_sine,
        forge_noise_white, forge_noise_pink, forge_noise_brown,
        forge_noise_crackle, forge_noise_digital,
        forge_noise_white_full, forge_noise_pink_full, forge_noise_brown_full,
        forge_noise_crackle_full, forge_noise_digital_full,
        forge_flt_off, forge_flt_lp, forge_flt_hp, forge_flt_bp,
        // Plugin wrapper and plugin settings
        plg_opening_fl, plg_fl_output_taken, plg_fl_output_invalid, plg_fl_no_state, plg_page_plugin, plg_page_params, plg_page_settings, plg_crashed, plg_failed, plg_reload, plg_save_fx_preset, plg_load_fx_preset, plg_piano_roll_f7, plg_close_separate, plg_editor_separate, plg_outputs_hint, plg_outputs, plg_map_to_inserts, plg_map_to_inserts_tip, plg_unroute, plg_unroute_tip, plg_with_1_2, plg_route_each_pair, plg_card_channel, plg_mixer_insert, plg_mixer_insert_hint, plg_vol_pan, plg_k_volume, plg_k_volume_hint, plg_k_pan, plg_k_pan_hint, plg_stuck_notes, plg_stuck_notes_hint, plg_notes_off, plg_card_mixer_slot, plg_place, plg_on, plg_on_hint, plg_mix, plg_mix_hint, plg_mix_knob_hint, plg_card_processing, plg_process, plg_process_bridged, plg_process_inside, plg_latency, plg_latency_none, plg_change_in_settings, plg_card_state, plg_file, plg_saved_state, plg_saved_state_hint, plg_none_yet, plg_reload_tip, plg_reset_defaults, plg_reset_defaults_tip, plg_not_loaded_dot, plg_flex_standalone, plg_no_editor, plg_wayland_note, plg_hide_editor, plg_show_editor, plg_filter_params, plg_off, plg_st_not_loaded, plg_st_loading, plg_st_editor_open, plg_st_ready, plg_card_safety, plg_bridged, plg_bridged_hint, plg_search, plg_kind_all, plg_kind_instruments, plg_kind_effects, plg_kind_problems, plg_scanning, plg_rescan_all, plg_col_name, plg_col_type, plg_col_vendor, plg_col_status, plg_list_empty, plg_nothing_matches, plg_st_hidden, plg_st_hangs, plg_st_crashes, plg_instrument, plg_effect, plg_card_selected, plg_unknown_vendor, plg_via_yabridge, plg_no_own_editor, plg_add_to_rack, plg_show_in_folder, plg_probing, plg_rescan_this, plg_rescan_this_tip, plg_show_in_lists, plg_hide_from_lists, plg_hide_tip, plg_failed_test, plg_try_rack, plg_missing_hint, plg_card_windows, plg_system_wine, plg_wine_hint, plg_wine_from_path, plg_other, plg_other_tip, plg_no_wine, plg_dxvk_hint, plg_stop_it, plg_stop_it_tip, plg_wine_stopped, plg_card_folders, plg_missing, plg_add_folder, plg_paths_note, plg_no_presets, plg_preset, plg_fl_live_link, plg_link_not_running, plg_link_starting, plg_link_connected, plg_link_check, plg_link_error, plg_link_about, plg_link_not_measured, plg_link_chan_off, plg_link_no_sound, plg_link_sound_in, plg_link_running, plg_link_not_running_lc, plg_link_notes, plg_link_port_ready, plg_link_port_missing, plg_link_fl_listens, plg_link_input_open, plg_link_restart_fl, plg_link_sound, plg_link_plays_here, plg_link_no_sink, plg_link_flow, plg_link_delay, plg_link_gaps, plg_open_fl, plg_starting_fl, plg_reconnect, plg_reconnect_tip, plg_reconnecting, plg_flex_ready, plg_flex_needs, plg_yes, plg_no, plg_fl_not_found, plg_system_wine_missing, plg_fl_cert_note, plg_flex_not_vsti, plg_link_howto, plg_link_no_export, plg_reloading, plg_checking, plg_scan_done, plg_fl_connected, plg_audio_busy, plg_reload_err_tip, plg_outputs_summary, plg_mapped, plg_place_slot, plg_latency_buffer, plg_state_bytes, plg_reset_done, plg_n_params, plg_no_editor_window, plg_link_exists, plg_loading_name, plg_counts, plg_add_to, plg_try_add_to, plg_other_wine, plg_stop_wine_err, plg_no_saved_presets, plg_saved_presets, plg_link_delay_val, plg_link_last_sound, plg_link_gaps_val, plg_flex_facts,
        // Project info, unsaved changes, autosave
        proj_recovered, proj_info_title, proj_title, proj_author, proj_author_hint, proj_genre, proj_genre_hint, proj_comments, proj_comments_hint, proj_mp3_tags, proj_before_new, proj_before_open, proj_before_quit, proj_unsaved_title, proj_dont_save, proj_not_saved, proj_just_now, proj_recover_autosave, proj_save_before, proj_recover_autosave_when,
        // Audio export
        exp_audio_filter, exp_waiting_fl, exp_rendering, exp_clipped, exp_cancelled, exp_title, exp_sec_file, exp_change, exp_sec_source, exp_empty_playlist, exp_sec_tail, exp_sec_formats, exp_off, exp_needs_lame, exp_tags_none, exp_tag_title, exp_tag_author, exp_tag_album, exp_tag_genre, exp_tag_year, exp_tag_copyright, exp_tag_url, exp_tag_comments, exp_sec_options, exp_normalize, exp_dither, exp_dither_only16, exp_split_tip, exp_start, exp_choose_format, exp_tail_leave, exp_tail_cut, exp_tail_wrap, exp_tail_leave_hint, exp_tail_cut_hint, exp_tail_wrap_hint, exp_rendering_pct, exp_writing, exp_done, exp_in_folder, exp_src_pattern, exp_src_song, exp_src_selection, exp_tags, exp_size,
        // Automation labels and Multilink
        auto_effect, auto_parameter, auto_plugin, auto_ml_armed, auto_ml_cancelled, auto_ml_tweak_first, auto_switch_song, auto_l_volume, auto_l_pan, auto_l_fx_mix, auto_l_fx_on, auto_l_send, auto_l_fx_param, auto_l_plugin_param, auto_copied, auto_pasted, auto_reset, auto_ml_complete, auto_ml_linked_next, auto_ml_queued, auto_rec_one, auto_rec_done, auto_clip_created,
        // MIDI import and export
        midi_limiter_added, midi_cannot_read, midi_cannot_import, midi_exported_song, midi_exported_pattern, midi_not_written,
        // Recording
        rec_press_play, rec_turn_on, rec_midi_note, rec_auto_note, rec_auto_on, rec_auto_off, rec_default_input, rec_input_disarmed, rec_nothing, rec_disarmed, rec_max_taps, rec_armed, rec_off, rec_on_none, rec_input_armed, rec_input_err, rec_saved, rec_take_err,
        // Tempo lane
        tl_open, tl_fold, tl_caps, tl_moved, tl_ramps, tl_jumps, tl_between, tl_first_stays, tl_deleted, tl_reset, tl_half, tl_double, tl_scaled, tl_at, tl_point_at, tl_cleared,
        // Presets
        pk_channel, pk_mixer, pk_fx, pk_synth, pre_saved, pre_save_err, pre_insert_n, pre_synth_on, pre_synth_added, pre_not_preset, pre_ch_loaded, pre_ch_added, pre_mixer_on, pre_fx_on,
        // Riff Machine
        riff_progression, riff_changes, riff_chord_voices, riff_inversion, riff_rhythm, riff_direction, riff_division, riff_octaves, riff_continue, riff_reverse, riff_invert, riff_velocity, riff_pan_spread, riff_release, riff_fine, riff_mod_x, riff_mod_y, riff_length_pct, riff_variation_pct, riff_grid, riff_timing, riff_keep_length, riff_keep_end, riff_snap_end, riff_grid_strength, riff_swing, riff_jitter, riff_key, riff_scale, riff_lowest, riff_highest, riff_recipes, riff_reset_all, riff_save_recipe, riff_recipe_filter, riff_recipe_saved, riff_load_recipe, riff_recipe_loaded, riff_recipe_range, riff_vel_caps, riff_select_first, riff_source_updated, riff_throw_dice, riff_stop_preview, riff_play_preview, riff_preview_needs, riff_apply, riff_stages_title, riff_length, riff_bars_suffix, riff_seed, riff_use_existing, riff_phrase, riff_through_stage, riff_enabled, riff_reset, riff_vary, riff_st_progression, riff_st_chords, riff_st_arpeggio, riff_st_mirror, riff_st_levels, riff_st_articulation, riff_st_groove, riff_st_fit, riff_seeded_walk, riff_triplet, riff_rh_straight, riff_rh_tresillo, riff_rh_offbeat, riff_rh_gallop, riff_dir_up, riff_dir_down, riff_dir_updown, riff_dir_random, riff_save_err, riff_load_err, riff_counts,
        // Settings window
        set_pg_audio, set_pg_midi, set_pg_plugins, set_pg_general, set_pg_audio_blurb, set_pg_midi_blurb, set_pg_plugins_blurb, set_pg_general_blurb, set_au_preferred_back, set_au_after_error, set_au_after_stall, set_au_after_disconnect, set_link_cancelled, set_typing_on, set_typing_off, set_metronome_on, set_metronome_off, set_pattern_full, set_apply_note, set_all_notes_off, set_card_output, set_device, set_system_default, set_sample_rate, set_buffer, set_buffer_hint, set_auto, set_running, set_device_buffer, set_chosen_output, set_unknown, set_no_output, set_restart_audio, set_card_input, set_record_input, set_record_input_hint, set_card_midi_in, set_midi_in_off, set_no_midi_in, set_connected, set_off_lc, set_rescan, set_panic, set_activity, set_keys_play, set_card_midi_out, set_midi_out_off, set_no_midi_out, set_midi_clock, set_midi_clock_hint, set_clock_need_port, set_card_notes_in, set_channel, set_velocity, set_level, set_transpose, set_reset, set_sustain, set_sustain_hint, set_card_recording, set_quantize, set_quantize_hint, set_record_note, set_card_links, set_no_links, set_forget_link, set_links_note, set_card_interface, set_size, set_size_hint, set_theme, set_theme_sys_light, set_theme_sys_dark, set_theme_sys_none, set_theme_hint, set_deck_monitor, set_deck_monitor_hint, set_x11, set_x11_hint, set_title_bar, set_title_bar_hint, set_card_projects, set_autosave, set_off, set_starter, set_starter_hint, set_ask_close, set_ask_close_hint, set_card_updates, set_check_auto, set_check_auto_hint, set_current_version, set_checking, set_check_now, set_default_buffer, set_vc_linear, set_vc_soft, set_vc_hard, set_vc_fixed, set_clk_send, set_clk_receive, set_dm_scope, set_dm_spectrum, set_th_dark, set_th_light, set_th_system, set_not_saved, set_audio_ok, set_audio_failed, set_au_recovered, set_audio_lost, set_learn, set_learned, set_frames, set_latency_line, set_rate_mismatch, set_fallback, set_stream_error, set_reconnecting, set_last_error, set_activity_last, set_clock_sending, set_semitones, set_system_scale, set_clear_skipped,
        // Effect faces
        fxv_eq_tip, fxv_low_shelf, fxv_high_shelf, fxv_out_db, fxv_in_db, fxv_decay_approx, fxv_bloom_balanced, fxv_bloom_warm, fxv_bloom_presence, fxv_bloom_dense, fxv_amount, fxv_amount_hint, fxv_loop, fxv_steps_suffix, fxv_clear, fxv_clear_tip, fxv_dice, fxv_dice_tip, fxv_time_note, fxv_smooth_hint, fxv_slice_hint, fxv_low, fxv_mid, fxv_high, fxv_threshold, fxv_ratio, fxv_threshold_hint, fxv_ratio_hint, fxv_type, fxv_delay, fxv_mod, fxv_move, fxv_lines, fxv_relation, fxv_rel_none, fxv_rel_equal, fxv_rel_double, fxv_rel_half, fxv_pre, fxv_solo, fxv_solo_off, fxv_side, fxv_solo_mid, fxv_solo_side, fxv_hit, fxv_body, fxv_impulse, fxv_tm_play, fxv_tm_half, fxv_tm_double, fxv_tm_reverse, fxv_tm_stutter, fxv_tm_gate, fxv_th_play, fxv_th_half, fxv_th_double, fxv_th_reverse, fxv_th_stutter, fxv_th_gate, fxv_band_n, fxv_steps_per_tap, fxv_step_tip, fxv_bands,
        // Automation clip editor
        ae_control_gone, ae_longer, ae_shorter, ae_help, ae_on, ae_off, ae_title,
        // File browser
        fb_sort_name, fb_sort_type, fb_sort_newest, fb_sort_largest, fb_add_folder, fb_add_folder_tip, fb_refresh, fb_refresh_tip, fb_sort_tip, fb_no_folders, fb_fav_remove, fb_fav_add, fb_load_selected, fb_load_new, fb_remove_root, fb_copy_path, fb_sort_caps, fb_searching, fb_drag, fb_previewing,
        // Effect response panels
        fxr_autopan, fxr_tremolo, fxr_ringmod, fxr_phaser, fxr_sh, fxr_fold, fxr_shelf, fxr_gate, fxr_transient,
        // Recent projects
        proj_no_autosave, proj_no_recent, proj_clear_list,
        // MIDI file dropped on a channel
        midi_no_notes,
        // Updater window
        upd_idle, upd_check_daily, upd_opened_page, upd_install_tip, upd_browser_err, upd_copied, upd_skipped,
        // File browser placeholders and audio input errors
        fb_reading, rec_no_input,
        //
        fb_unreadable, rec_no_float,
        // Demo / showcase mode
        view_demo_mode, show_arrange, show_notes, show_play, show_pause, show_stop,
        show_all_channels, show_selected_channel, show_range_song, show_range_loop,
        show_hud, show_fullscreen, show_record, show_recording, show_cancel, show_exit,
        show_empty_arrange, show_empty_notes,
        show_mp4_filter, show_rec_cancelled, show_rec_audio_write_err, show_rec_resized_err,
        show_rec_encoder_stopped, show_rec_screenshot_timeout, show_rec_ffmpeg_stdin,
        show_rec_ffmpeg_write_err, show_rec_ffmpeg_wait_err, show_rec_ffmpeg_err,
        show_rec_saved_video, show_rec_frames_dir_err, show_rec_png_save_err,
        // Project info (RC2 3-tab window)
        info_window_title, info_tab_details, info_tab_notes, info_tab_settings,
        info_title, info_author, info_author_hint, info_album, info_album_hint,
        info_genre, info_genre_hint, info_year, info_year_hint, info_copyright, info_copyright_hint,
        info_url, info_url_hint, info_url_open, info_url_invalid, info_url_open_err,
        info_url_opened, info_license_all_rights, info_from_file, info_comments,
        info_comments_hint, info_copy, info_clear, info_show_on_open, info_show_on_open_hint,
        info_tags_heading, info_tags_empty, info_open_export, info_close,
        info_set_tempo, info_set_tempo_map_note, info_set_swing, info_set_meter,
        info_set_key, info_set_key_off, info_set_snap_scale,
        info_set_adopt_roll, info_set_game_style, info_set_game_style_none, info_edit_from_export,
        // Browser & file tree (RC2)
        browser_stop_preview, browser_tab_all, browser_tab_library, browser_tab_files,
        browser_dismiss_warning, fb_drag_reorder,
        // Unified import dialog (RC2)
        imp_title, imp_choose_file, imp_change_file, imp_browse, imp_destination,
        imp_dest_flp, imp_dest_bundle, imp_dest_midi, imp_dest_new_sampler,
        imp_dest_selected_sampler, imp_file_unavailable, imp_supported_hint,
        imp_action, imp_filter_all, imp_filter_audio, imp_filter_project, imp_unsupported_err,
        // Tap tempo window
        tap_pad_hint, tap_pad_empty, tap_taps, tap_inputs, tap_average, tap_average_tip, tap_rounding, tap_round_whole, tap_auto_apply, tap_keys, tap_keys_tip, tap_midi, tap_midi_tip, tap_apply, tap_reset, tap_project_tempo,
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
        smp_global_edit_zones,
        smp_zones_count,
        smp_frames_count,
        vocal_syllables,
        vocal_notes_in_channel,
        vocal_notes_selected,
        vocal_lyrics_set,
        tide_unison_voices,
        forge_exported_files,
        proj_min_ago,
        proj_h_ago,
        proj_days_ago,
        exp_stem_files,
        exp_split,
        auto_ml_finished,
        auto_rec_many,
        midi_loading_sounds,
        rec_on_n,
        rec_frames_lost,
        rec_takes,
        lib_kit_added,
        riff_added,
        ae_bars,
        fb_matches,
        midi_notes_into,
        show_rec_saved_frames,
        info_stat_channels,
        info_stat_patterns,
        info_stat_clips,
        info_stat_plugins,
        info_stat_missing_plugins,
        info_stat_bars,
        info_set_meter_changes,
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

    pub fn drum_kit_label(&self, kit: keyframexl::kits::Kit) -> &'static str {
        match kit {
            keyframexl::kits::Kit::Core => self.inst_kit_core,
            keyframexl::kits::Kit::Chip => self.inst_kit_chip,
        }
    }

    pub fn smp_pages(&self) -> [&'static str; 5] {
        [
            self.smp_page_program,
            self.smp_page_zone,
            self.smp_page_filter,
            self.smp_page_mod,
            self.smp_page_zones,
        ]
    }

    pub fn smp_loop_types(&self) -> [&'static str; 5] {
        [
            self.smp_loop_off,
            self.smp_loop_fwd,
            self.smp_loop_oneshot,
            self.smp_loop_sustained,
            self.smp_loop_bounce,
        ]
    }

    pub fn smp_play_mode(&self, m: keyframexl::sampler::PlayMode) -> &'static str {
        use keyframexl::sampler::PlayMode as P;
        match m {
            P::Poly => self.smp_play_poly,
            P::Mono => self.smp_play_mono,
            P::Legato => self.smp_play_legato,
        }
    }

    pub fn smp_glide_mode(&self, m: keyframexl::sampler::GlideMode) -> &'static str {
        use keyframexl::sampler::GlideMode as G;
        match m {
            G::Fixed => self.smp_glide_fixed,
            G::Auto => self.smp_glide_auto,
        }
    }

    pub fn smp_stretch_mode(&self, m: keyframexl::sampler::StretchMode) -> &'static str {
        use keyframexl::sampler::StretchMode as S;
        match m {
            S::Off => self.smp_stretch_off,
            S::Time => self.smp_stretch_time,
            S::Beat => self.smp_stretch_beat,
        }
    }

    pub fn smp_filter_kind(&self, k: keyframexl::sampler::FilterKind) -> &'static str {
        use keyframexl::sampler::FilterKind as F;
        match k {
            F::Off => self.smp_fk_off,
            F::Lowpass => self.smp_fk_lp,
            F::Highpass => self.smp_fk_hp,
            F::Bandpass => self.smp_fk_bp,
            F::Notch => self.smp_fk_notch,
            F::Allpass => self.smp_fk_ap,
        }
    }

    pub fn smp_filter_routing(&self, r: keyframexl::sampler::FilterRouting) -> &'static str {
        use keyframexl::sampler::FilterRouting as R;
        match r {
            R::Serial => self.smp_fr_serial,
            R::Parallel => self.smp_fr_parallel,
        }
    }

    pub fn smp_lfo_shape(&self, s: keyframexl::sampler::LfoShape) -> &'static str {
        use keyframexl::sampler::LfoShape as L;
        match s {
            L::Sine => self.smp_lfo_sine,
            L::AbsSine => self.smp_lfo_abs_sine,
            L::Triangle => self.smp_lfo_tri,
            L::Square => self.smp_lfo_sq,
            L::Saw => self.smp_lfo_saw,
            L::InvSaw => self.smp_lfo_inv_saw,
            L::Random => self.smp_lfo_rand,
            L::LpRandom => self.smp_lfo_lp_rand,
        }
    }

    pub fn smp_mod_source(&self, s: keyframexl::sampler::ModSource) -> &'static str {
        use keyframexl::sampler::ModSource as M;
        match s {
            M::None => "—",
            M::Lfo1 => "LFO 1",
            M::Lfo2 => "LFO 2",
            M::Env1 => self.smp_ms_env1,
            M::Env2 => self.smp_ms_env2,
            M::Velocity => self.smp_ms_vel,
            M::Key => self.smp_ms_key,
            M::Random => self.smp_ms_rand,
            M::ProgLfo1 => self.smp_ms_plfo1,
            M::ProgLfo2 => self.smp_ms_plfo2,
        }
    }

    pub fn smp_mod_target(&self, t: keyframexl::sampler::ModTarget) -> &'static str {
        use keyframexl::sampler::ModTarget as M;
        match t {
            M::Pitch => self.smp_mt_pitch,
            M::Gain => self.smp_mt_gain,
            M::Pan => self.smp_mt_pan,
            M::Cutoff1 => self.smp_mt_cut1,
            M::Emphasis1 => self.smp_mt_emph1,
            M::Cutoff2 => self.smp_mt_cut2,
            M::Emphasis2 => self.smp_mt_emph2,
            M::Lfo1Rate => self.smp_mt_lfo1,
            M::Lfo2Rate => self.smp_mt_lfo2,
        }
    }

    pub fn tide_table(&self, v: keyframexl::synth::Table) -> &'static str {
        use keyframexl::synth::Table as T;
        match v {
            T::SineSaw => self.tide_tbl_sine_saw,
            T::SquarePulse => self.tide_tbl_square,
            T::Organ => self.tide_tbl_organ,
            T::Vowel => self.tide_tbl_vowel,
        }
    }

    pub fn tide_warp(&self, v: keyframexl::synth::Warp) -> &'static str {
        use keyframexl::synth::Warp as W;
        match v {
            W::Bend => self.tide_wrp_bend,
            W::Sync => self.tide_wrp_sync,
            W::PwmMirror => self.tide_wrp_mirror,
            W::FmSub => self.tide_wrp_fm_sub,
        }
    }

    pub fn tide_param(&self, v: keyframexl::synth::Param) -> &'static str {
        use keyframexl::synth::Param as P;
        match v {
            P::Position => self.tide_p_position,
            P::Warp => self.tide_p_warp,
            P::Cutoff => self.tide_p_cutoff,
            P::Resonance => self.tide_p_resonance,
            P::Sub => self.tide_p_sub,
            P::Noise => self.tide_p_noise,
            P::Detune => self.tide_p_detune,
            P::Spread => self.tide_p_spread,
        }
    }

    pub fn tide_param_short(&self, v: keyframexl::synth::Param) -> &'static str {
        use keyframexl::synth::Param as P;
        match v {
            P::Position => self.tide_ps_position,
            P::Warp => self.tide_ps_warp,
            P::Cutoff => self.tide_ps_cutoff,
            P::Resonance => self.tide_ps_resonance,
            P::Sub => self.tide_ps_sub,
            P::Noise => self.tide_ps_noise,
            P::Detune => self.tide_ps_detune,
            P::Spread => self.tide_ps_spread,
        }
    }

    pub fn tide_mod_source(&self, s: keyframexl::synth::ModSource) -> &'static str {
        use keyframexl::synth::ModSource as M;
        match s {
            M::Lfo1 => "LFO 1",
            M::Lfo2 => "LFO 2",
            M::FilterEnv => self.tide_ms_filter_env,
            M::Velocity => self.tide_ms_velocity,
            M::Macro(0) => self.tide_ms_macro_1,
            M::Macro(1) => self.tide_ms_macro_2,
            M::Macro(2) => self.tide_ms_macro_3,
            M::Macro(3) => self.tide_ms_macro_4,
            M::Macro(_) => self.tide_ms_invalid_macro,
            M::ModWheel => self.tide_ms_mod_wheel,
            M::Pressure => self.tide_ms_pressure,
            M::Slide => self.tide_ms_slide,
        }
    }

    pub fn tide_mod_target(&self, t: keyframexl::synth::ModTarget) -> &'static str {
        use keyframexl::synth::ModTarget as M;
        match t {
            M::Position => self.tide_p_position,
            M::Warp => self.tide_p_warp,
            M::Cutoff => self.tide_p_cutoff,
            M::Resonance => self.tide_p_resonance,
            M::Sub => self.tide_p_sub,
            M::Noise => self.tide_p_noise,
            M::Detune => self.tide_p_detune,
            M::Spread => self.tide_p_spread,
            M::PositionB => self.tide_mt_pos_b,
            M::WarpB => self.tide_mt_warp_b,
            M::LevelB => self.tide_mt_level_b,
            M::PitchA => self.tide_mt_pitch_a,
            M::PitchB => self.tide_mt_pitch_b,
            M::LevelC => self.tide_mt_level_c,
            M::TiltC => self.tide_mt_tilt_c,
            M::OddC => self.tide_mt_odd_c,
        }
    }

    pub fn tide_lfo_shapes(&self) -> [&'static str; 6] {
        [
            self.tide_shp_off,
            self.tide_shp_sine,
            self.tide_shp_tri,
            self.tide_shp_saw,
            self.tide_shp_sq,
            self.tide_shp_sh,
        ]
    }

    pub fn forge_style(&self, s: keyframexl::forge::recipes::Style) -> &'static str {
        use keyframexl::forge::recipes::Style as S;
        match s {
            S::Retro => self.forge_style_retro,
            S::Cartoon => self.forge_style_cartoon,
            S::Heavy => self.forge_style_heavy,
            S::SciFi => self.forge_style_scifi,
        }
    }

    pub fn forge_style_hint(&self, s: keyframexl::forge::recipes::Style) -> &'static str {
        use keyframexl::forge::recipes::Style as S;
        match s {
            S::Retro => self.forge_style_retro_tip,
            S::Cartoon => self.forge_style_cartoon_tip,
            S::Heavy => self.forge_style_heavy_tip,
            S::SciFi => self.forge_style_scifi_tip,
        }
    }

    pub fn forge_game_styles(&self) -> [&'static str; 5] {
        [
            self.forge_style_any,
            self.forge_style_retro,
            self.forge_style_cartoon,
            self.forge_style_heavy,
            self.forge_style_scifi,
        ]
    }

    pub fn forge_macros(&self) -> [(&'static str, &'static str); 4] {
        [
            (self.forge_macro_size, self.forge_macro_size_hint),
            (self.forge_macro_energy, self.forge_macro_energy_hint),
            (self.forge_macro_dirt, self.forge_macro_dirt_hint),
            (self.forge_macro_space, self.forge_macro_space_hint),
        ]
    }

    pub fn forge_category(&self, c: keyframexl::forge::library::Category) -> &'static str {
        use keyframexl::forge::library::Category as C;
        match c {
            C::Ui => self.forge_cat_ui,
            C::Steps => self.forge_cat_steps,
            C::Actions => self.forge_cat_actions,
            C::Weapons => self.forge_cat_weapons,
            C::Pickups => self.forge_cat_pickups,
            C::Magic => self.forge_cat_magic,
            C::Damage => self.forge_cat_damage,
            C::Doors => self.forge_cat_doors,
            C::Ambience => self.forge_cat_ambience,
            C::Creatures => self.forge_cat_creatures,
        }
    }

    pub fn forge_wave(&self, w: keyframexl::forge::Wave) -> &'static str {
        use keyframexl::forge::Wave as W;
        match w {
            W::Square => self.forge_wave_square,
            W::Pulse25 => self.forge_wave_pulse25,
            W::Pulse12 => self.forge_wave_pulse12,
            W::Triangle => self.forge_wave_tri,
            W::Saw => self.forge_wave_saw,
            W::Sine => self.forge_wave_sine,
        }
    }

    pub fn forge_noise_color(&self, c: keyframexl::forge::NoiseColor) -> &'static str {
        use keyframexl::forge::NoiseColor as N;
        match c {
            N::White => self.forge_noise_white,
            N::Pink => self.forge_noise_pink,
            N::Brown => self.forge_noise_brown,
            N::Crackle => self.forge_noise_crackle,
            N::Digital => self.forge_noise_digital,
        }
    }

    pub fn forge_noise_full(&self, c: keyframexl::forge::NoiseColor) -> &'static str {
        use keyframexl::forge::NoiseColor as N;
        match c {
            N::White => self.forge_noise_white_full,
            N::Pink => self.forge_noise_pink_full,
            N::Brown => self.forge_noise_brown_full,
            N::Crackle => self.forge_noise_crackle_full,
            N::Digital => self.forge_noise_digital_full,
        }
    }

    pub fn forge_filter_mode(&self, m: keyframexl::forge::FilterMode) -> &'static str {
        use keyframexl::forge::FilterMode as F;
        match m {
            F::Off => self.forge_flt_off,
            F::LowPass => self.forge_flt_lp,
            F::HighPass => self.forge_flt_hp,
            F::BandPass => self.forge_flt_bp,
        }
    }

    pub fn export_tail(&self, t: keyframexl::render::Tail) -> &'static str {
        use keyframexl::render::Tail as T;
        match t {
            T::Leave => self.exp_tail_leave,
            T::Cut => self.exp_tail_cut,
            T::Wrap => self.exp_tail_wrap,
        }
    }

    pub fn export_tail_hint(&self, t: keyframexl::render::Tail) -> &'static str {
        use keyframexl::render::Tail as T;
        match t {
            T::Leave => self.exp_tail_leave_hint,
            T::Cut => self.exp_tail_cut_hint,
            T::Wrap => self.exp_tail_wrap_hint,
        }
    }

    pub fn preset_kind(&self, k: keyframexl::preset::Kind) -> &'static str {
        use keyframexl::preset::Kind as K;
        match k {
            K::Channel => self.pk_channel,
            K::Mixer => self.pk_mixer,
            K::Fx => self.pk_fx,
            K::Synth => self.pk_synth,
        }
    }

    pub fn riff_stages(&self) -> [&'static str; 8] {
        [
            self.riff_st_progression,
            self.riff_st_chords,
            self.riff_st_arpeggio,
            self.riff_st_mirror,
            self.riff_st_levels,
            self.riff_st_articulation,
            self.riff_st_groove,
            self.riff_st_fit,
        ]
    }

    /// Roman-numeral progressions stay as written; only the seeded walk is a word.
    pub fn riff_progressions(&self) -> [&'static str; 5] {
        let p = keyframexl::riff::PROGRESSIONS;
        [p[0], p[1], p[2], p[3], self.riff_seeded_walk]
    }

    pub fn riff_rates(&self) -> [&'static str; 5] {
        let r = keyframexl::riff::RATES;
        [r[0].0, r[1].0, self.riff_triplet, r[3].0, r[4].0]
    }

    pub fn riff_rhythms(&self) -> [&'static str; 4] {
        [
            self.riff_rh_straight,
            self.riff_rh_tresillo,
            self.riff_rh_offbeat,
            self.riff_rh_gallop,
        ]
    }

    pub fn riff_directions(&self) -> [&'static str; 4] {
        [
            self.riff_dir_up,
            self.riff_dir_down,
            self.riff_dir_updown,
            self.riff_dir_random,
        ]
    }

    pub fn velocity_curve(&self, c: crate::settings::VelocityCurve) -> &'static str {
        use crate::settings::VelocityCurve as V;
        match c {
            V::Linear => self.set_vc_linear,
            V::Soft => self.set_vc_soft,
            V::Hard => self.set_vc_hard,
            V::Fixed => self.set_vc_fixed,
        }
    }

    pub fn midi_clock_mode(&self, m: crate::settings::MidiClockMode) -> &'static str {
        use crate::settings::MidiClockMode as C;
        match m {
            C::Off => self.set_off,
            C::Master => self.set_clk_send,
            C::Slave => self.set_clk_receive,
        }
    }

    pub fn deck_monitor(&self, m: crate::settings::DeckMonitor) -> &'static str {
        use crate::settings::DeckMonitor as D;
        match m {
            D::Off => self.set_off,
            D::Scope => self.set_dm_scope,
            D::Spectrum => self.set_dm_spectrum,
        }
    }

    pub fn theme_mode(&self, m: crate::settings::ThemeMode) -> &'static str {
        use crate::settings::ThemeMode as T;
        match m {
            T::Dark => self.set_th_dark,
            T::Light => self.set_th_light,
            T::System => self.set_th_system,
        }
    }

    pub fn fx_time_modes(&self) -> [&'static str; 6] {
        [
            self.fxv_tm_play,
            self.fxv_tm_half,
            self.fxv_tm_double,
            self.fxv_tm_reverse,
            self.fxv_tm_stutter,
            self.fxv_tm_gate,
        ]
    }

    pub fn fx_time_hints(&self) -> [&'static str; 6] {
        [
            self.fxv_th_play,
            self.fxv_th_half,
            self.fxv_th_double,
            self.fxv_th_reverse,
            self.fxv_th_stutter,
            self.fxv_th_gate,
        ]
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
