#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
#[cfg(target_os = "linux")]
mod bridge;
#[cfg(target_os = "linux")]
mod clap;
mod dwp;
#[cfg(target_os = "linux")]
mod fl_link;
#[cfg(target_os = "linux")]
mod fl_native;
#[cfg(target_os = "linux")]
mod flp_freeze;
mod i18n;
mod import;
mod midi_in;
mod midi_out;
mod platform;
#[cfg(target_os = "linux")]
mod plugins;
mod settings;
mod theme;
mod ui;
mod updater;
#[cfg(target_os = "linux")]
mod vst2;

#[cfg(not(target_os = "linux"))]
use platform::stubs::{bridge, fl_link, fl_native, flp_freeze, plugins};

use std::{env, error::Error};
#[cfg(target_os = "linux")]
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn main() -> Result<(), Box<dyn Error>> {
    platform::prepare_process();
    updater::cleanup_old_binary();
    let args: Vec<String> = env::args().collect();
    let is_gui_launch = args.len() <= 3
        && args.get(1).is_none_or(|a| {
            a == "--demo" || a == "--show" || a.starts_with("--capture-") || is_project_arg(a)
        })
        && args.get(2).is_none_or(|a| {
            a == "--show"
                || args
                    .get(1)
                    .is_some_and(|first| first.starts_with("--capture-"))
        });
    if !is_gui_launch {
        platform::attach_parent_console();
    }
    match args.get(1).map(String::as_str) {
        // A bridged plugin's own process (started by the program, never by hand).
        Some("plugin-host") => {
            bridge::child_main()?;
        }
        // Developer check: import every .dwp under a folder and report failures.
        Some("import-check") if args.len() == 3 => {
            let mut files = Vec::new();
            let mut stack = vec![std::path::PathBuf::from(&args[2])];
            while let Some(dir) = stack.pop() {
                for e in std::fs::read_dir(&dir)?.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dwp")) {
                        files.push(p);
                    }
                }
            }
            files.sort();
            let (mut ok, mut bad) = (0, 0);
            for f in &files {
                match import::dwp(f) {
                    Ok(p) => {
                        ok += 1;
                        println!("OK   {} · {} zones", f.display(), p.zones.len());
                    }
                    Err(e) => {
                        bad += 1;
                        println!("FAIL {} · {e}", f.display());
                    }
                }
            }
            println!("{ok} loaded, {bad} failed, {} total", files.len());
        }
        #[cfg(target_os = "linux")]
        Some("vst2-probe") if args.len() == 3 => {
            if std::env::var_os("KFXL_PROBE_WINE_SET").is_none() {
                // Run under the Wine chosen in Settings, as plugins do.
                let prefs = settings::Settings::load();
                plugins::set_dxvk(prefs.plugins.dxvk);
                plugins::set_wine(prefs.plugins.wine.clone());
                let mut c = std::process::Command::new(env::current_exe()?);
                c.args(&args[1..]).env("KFXL_PROBE_WINE_SET", "1");
                plugins::apply_wine_for(&mut c, std::path::Path::new(&args[2]));
                std::process::exit(c.status()?.code().unwrap_or(1));
            }
            vst2::probe(std::path::Path::new(&args[2])).map_err(std::io::Error::other)?;
        }
        // Live check of the FL Studio link: start FL (with an optional project),
        // play a note a second in real time, print what comes back.
        #[cfg(target_os = "linux")]
        Some("fl-link-test") if args.len() <= 3 => {
            let prefs = settings::Settings::load();
            plugins::set_dxvk(prefs.plugins.dxvk);
            plugins::set_wine(prefs.plugins.wine.clone());
            fl_link::fl_link_test(args.get(2).map(std::path::Path::new))
                .map_err(std::io::Error::other)?;
        }
        #[cfg(target_os = "linux")]
        Some("fl-native-test") if args.len() >= 4 => {
            let channels = args[3..]
                .iter()
                .map(|c| c.parse::<u32>())
                .collect::<Result<Vec<_>, _>>()?;
            fl_native::test(std::path::Path::new(&args[2]), &channels)
                .map_err(std::io::Error::other)?;
        }
        // FL Studio VSTi state for one channel of an FL project (for vst2-probe):
        // keyframexl flp-channel project.flp <channel> out.chunk
        Some("flp-channel") if args.len() == 5 => {
            let bytes = std::fs::read(&args[2])?;
            let index: u32 = args[3].parse().map_err(std::io::Error::other)?;
            let flp = keyframexl::flp::channel_flp(&bytes, index).map_err(std::io::Error::other)?;
            std::fs::write(&args[4], [keyframexl::flp::FL_VSTI_HEADER, &flp].concat())?;
        }
        // FL Studio renders FL project channels alone (FLEX): flp-freeze x.flp 3 5
        #[cfg(target_os = "linux")]
        Some("flp-freeze") if args.len() >= 4 => {
            let channels: Vec<u32> = args[3..].iter().filter_map(|a| a.parse().ok()).collect();
            flp_freeze::freeze_main(std::path::Path::new(&args[2]), &channels)
                .map_err(std::io::Error::other)?;
        }
        // Plugin scan: load one plugin in this throwaway process (see plugins.rs).
        Some("plugin-probe") if args.len() == 3 => {
            std::process::exit(plugins::probe_main(std::path::Path::new(&args[2])));
        }
        // Scan every VST3 folder now (ignores the cache) and print the verdicts.
        Some("plugin-scan") if args.len() == 2 => {
            let prefs = settings::Settings::load();
            plugins::set_dxvk(prefs.plugins.dxvk);
            plugins::set_wine(prefs.plugins.wine.clone());
            let list = plugins::scan(&prefs.plugins.extra_dirs, true, |d, n, name| {
                eprintln!("{d}/{n} {name}")
            });
            for p in &list {
                println!(
                    "{:<26} {:<5} {:?}  {}",
                    p.name,
                    if p.instrument { "inst" } else { "fx" },
                    p.status,
                    p.path.display()
                );
            }
        }
        // Live checks: screenshot the whole X11 display (plugin editors are X11 windows).
        #[cfg(target_os = "linux")]
        Some("x11-shot") if args.len() == 3 => {
            let (conn, screen) = xcb::Connection::connect(None)?;
            let root = conn
                .get_setup()
                .roots()
                .nth(screen as usize)
                .ok_or("no screen")?;
            // XWayland has no root image: take the largest viewable top-level window.
            let tree = conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree {
                window: root.root(),
            }))?;
            let mut best: Option<(xcb::x::Window, u16, u16)> = None;
            for &win in tree.children() {
                let attr = conn.wait_for_reply(
                    conn.send_request(&xcb::x::GetWindowAttributes { window: win }),
                )?;
                if attr.map_state() != xcb::x::MapState::Viewable
                    || attr.class() != xcb::x::WindowClass::InputOutput
                {
                    continue;
                }
                let g = conn.wait_for_reply(conn.send_request(&xcb::x::GetGeometry {
                    drawable: xcb::x::Drawable::Window(win),
                }))?;
                if best
                    .is_none_or(|b| g.width() as u32 * g.height() as u32 > b.1 as u32 * b.2 as u32)
                {
                    best = Some((win, g.width(), g.height()));
                }
            }
            let (win, w, h) = best.ok_or("no window")?;
            let img = conn.wait_for_reply(conn.send_request(&xcb::x::GetImage {
                format: xcb::x::ImageFormat::ZPixmap,
                drawable: xcb::x::Drawable::Window(win),
                x: 0,
                y: 0,
                width: w,
                height: h,
                plane_mask: u32::MAX,
            }))?;
            let rgb: Vec<u8> = img
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();
            image::RgbImage::from_raw(w as u32, h as u32, rgb)
                .ok_or("bad image")?
                .save(&args[2])?;
            println!("Wrote {}", args[2]);
        }
        // The showcase song as a project (and its demo voice beside the voicebanks).
        Some("write-demo") if args.len() == 3 => {
            let voice = keyframexl::demo::voice_dir();
            keyframexl::demo::write_voice(&voice)?;
            let song = keyframexl::demo::build(Some(&voice));
            keyframexl::project_io::save(&song, std::path::Path::new(&args[2]))?;
            println!("Wrote {} (voice: {})", args[2], voice.display());
        }
        // The showcase song rendered to a 48 kHz WAV.
        Some("render-demo-song") if args.len() == 3 => {
            let dir = std::env::temp_dir().join(format!("kfxl-demo-voice-{}", std::process::id()));
            keyframexl::demo::write_voice(&dir)?;
            let song = keyframexl::demo::build(Some(&dir));
            let audio = keyframexl::engine::render(&song, 1, true, 48_000);
            keyframexl::render::write_wav_at(
                std::path::Path::new(&args[2]),
                &audio,
                keyframexl::render::Format::Wav16,
                true,
                48_000,
            )?;
            let _ = std::fs::remove_dir_all(&dir);
            println!("Wrote {} ({:.1} s)", args[2], audio.len() as f64 / 48_000.0);
        }
        Some("render-demo") if args.len() == 3 => {
            keyframexl::write_wav(&args[2], &keyframexl::demo())?;
            println!("Wrote {}", args[2]);
        }
        #[cfg(target_os = "linux")]
        Some("play-demo") if args.len() == 2 => {
            let mut child = Command::new("pw-cat")
                .args([
                    "--playback",
                    "--raw",
                    "--rate",
                    "48000",
                    "--channels",
                    "2",
                    "--format",
                    "f32",
                    "-",
                ])
                .stdin(Stdio::piped())
                .spawn()?;
            let mut stdin = child.stdin.take().ok_or("pw-cat stdin unavailable")?;
            for frame in keyframexl::demo() {
                for sample in frame {
                    stdin.write_all(&sample.to_le_bytes())?;
                }
            }
            drop(stdin);
            if !child.wait()?.success() {
                return Err("pw-cat failed".into());
            }
        }
        _ if is_gui_launch => {
            if !args.iter().any(|a| a.starts_with("--capture-")) {
                platform::setup_log_file();
            }
            // Plugin editors (X11, also through yabridge) can only be placed inside
            // an X11 window: unless turned off, run on X11 (XWayland under Wayland).
            // Captures keep the default (the headless compositor has no XWayland).
            let capture = args.iter().any(|a| a.starts_with("--capture-"));
            let prefs = settings::Settings::load();
            // KFXL_CAPTURE_SIZE=WxH lets captures check narrow layouts.
            let size = env::var("KFXL_CAPTURE_SIZE")
                .ok()
                .and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
                })
                .unwrap_or_else(|| {
                    if capture {
                        settings::DEFAULT_WINDOW_SIZE
                    } else {
                        prefs.general.restored_window_size()
                    }
                });
            let x11 = env::var("KFXL_X11")
                .map(|v| v == "1")
                .unwrap_or(!capture && prefs.general.x11);
            if x11 && env::var_os("DISPLAY").is_some() {
                // SAFETY: still single-threaded here; winit reads it when the loop starts.
                unsafe { env::remove_var("WAYLAND_DISPLAY") };
            }
            let mut viewport = eframe::egui::ViewportBuilder::default()
                .with_app_id("keyframexl")
                .with_inner_size(size)
                .with_maximized(!capture && prefs.general.window_maximized)
                .with_decorations(!capture && prefs.general.system_title_bar)
                .with_min_inner_size(settings::MIN_WINDOW_SIZE);
            if let Some(icon) = app_icon() {
                viewport = viewport.with_icon(icon);
            }
            let options = eframe::NativeOptions {
                viewport,
                persist_window: false,
                ..Default::default()
            };
            eframe::run_native(
                "KeyFrameXL",
                options,
                Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
            )?;
            // The window is closed and the app has saved, stopped audio and
            // unloaded its plugins. Leave without the C library's exit
            // handlers: plugin libraries (yabridge among them) abort the
            // process when their statics are torn down under their own threads.
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            platform::exit_immediately(0);
        }
        _ => eprintln!(
            "Usage: keyframexl [project.kfxl | --demo | render-demo <file.wav> | play-demo]"
        ),
    }
    Ok(())
}

/// A project file operand (double-click / Open With): `song.kfxl`.
fn is_project_arg(a: &str) -> bool {
    let a = a.to_ascii_lowercase();
    !a.starts_with('-') && (a.ends_with(".kfxl") || a.ends_with(".flp"))
}

/// Embedded application icon (Step Matrix K) for the native window and taskbar.
fn app_icon() -> Option<eframe::egui::IconData> {
    let img = image::load_from_memory_with_format(
        include_bytes!("../assets/icon.png"),
        image::ImageFormat::Png,
    )
    .ok()?
    .into_rgba8();
    let (width, height) = img.dimensions();
    Some(eframe::egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}
