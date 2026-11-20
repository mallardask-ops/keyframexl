//! FL Studio renders native FL channels, each one alone and dry
//! (`flp::solo_channel_flp`), on a private X display so nothing pops up on the
//! desktop, its "Problems loading the project" box answered by a fake click.
//! The import then plays the WAV through that channel's mixer insert.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A private X server (Xvnc, no port): FL's windows and dialogs live there.
pub(crate) struct Display {
    child: Child,
    number: u32,
}

impl Display {
    pub(crate) fn start() -> Result<Self, String> {
        let number = (80..100)
            .find(|n| !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists())
            .ok_or("No free X display number")?;
        let child = Command::new("Xvnc")
            .arg(format!(":{number}"))
            .args([
                "-geometry",
                "1600x900",
                "-depth",
                "24",
                "-SecurityTypes",
                "None",
                "-rfbport",
                "-1",
            ])
            // Xvnc wants somewhere to listen: a socket nobody connects to.
            .arg("-rfbunixpath")
            .arg(std::env::temp_dir().join(format!("kfxl-vnc-{number}")))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                format!("Cannot start Xvnc (package tigervnc) for FL Studio's render: {e}")
            })?;
        let display = Self { child, number };
        let until = Instant::now() + Duration::from_secs(10);
        while !Path::new(&format!("/tmp/.X11-unix/X{number}")).exists() {
            if Instant::now() > until {
                return Err("Xvnc did not start".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(display)
    }

    pub(crate) fn name(&self) -> String {
        format!(":{}", self.number)
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // Let Xvnc remove its socket and display lock for the next session.
        unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Mapped top-level windows: (title, x, y, width, height).
pub(crate) fn windows(
    conn: &xcb::Connection,
    root: xcb::x::Window,
) -> Vec<(String, i16, i16, u16, u16)> {
    let Ok(tree) = conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree { window: root }))
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for &w in tree.children() {
        let Ok(a) =
            conn.wait_for_reply(conn.send_request(&xcb::x::GetWindowAttributes { window: w }))
        else {
            continue;
        };
        if a.map_state() != xcb::x::MapState::Viewable {
            continue;
        }
        let Ok(g) = conn.wait_for_reply(conn.send_request(&xcb::x::GetGeometry {
            drawable: xcb::x::Drawable::Window(w),
        })) else {
            continue;
        };
        let name = conn
            .wait_for_reply(conn.send_request(&xcb::x::GetProperty {
                delete: false,
                window: w,
                property: xcb::x::ATOM_WM_NAME,
                r#type: xcb::x::ATOM_ANY,
                long_offset: 0,
                long_length: 256,
            }))
            .map(|r| String::from_utf8_lossy(r.value::<u8>()).into_owned())
            .unwrap_or_default();
        out.push((name, g.x(), g.y(), g.width(), g.height()));
    }
    out
}

pub(crate) fn click(conn: &xcb::Connection, root: xcb::x::Window, x: i16, y: i16) {
    let fake = |r#type: u8, detail: u8| {
        conn.send_request(&xcb::xtest::FakeInput {
            r#type,
            detail,
            time: 0,
            root,
            root_x: x,
            root_y: y,
            deviceid: 0,
        });
        let _ = conn.flush();
    };
    fake(6, 0); // motion
    std::thread::sleep(Duration::from_millis(300));
    fake(4, 1); // press
    std::thread::sleep(Duration::from_millis(80));
    fake(5, 1); // release
}

/// FL renders `project` (a Windows path) to `<project>.wav` beside it.
fn render(
    bin: &Path,
    prefix: &Path,
    exe: &Path,
    display: &Display,
    project: &Path,
    timeout: Duration,
) -> Result<PathBuf, String> {
    let out = project.with_extension("wav");
    let _ = std::fs::remove_file(&out);
    let win = format!("Z:{}", project.display()).replace('/', "\\");
    let mut c = crate::fl_link::render_command(bin, prefix);
    c.arg(exe)
        .args(["/R", "/Ewav", &win])
        .env("DISPLAY", display.name())
        .env_remove("WAYLAND_DISPLAY")
        .env("WINEDEBUG", "-all")
        .current_dir(exe.parent().unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut fl = c
        .spawn()
        .map_err(|e| format!("Cannot start FL Studio: {e}"))?;
    let (conn, screen) = xcb::Connection::connect_with_extensions(
        Some(&display.name()),
        &[xcb::Extension::Test],
        &[],
    )
    .map_err(|e| format!("Cannot reach FL Studio's display: {e}"))?;
    let root = conn
        .get_setup()
        .roots()
        .nth(screen as usize)
        .ok_or("No screen")?
        .root();
    let start = Instant::now();
    let mut gone_since: Option<(Instant, u64)> = None;
    let result = loop {
        std::thread::sleep(Duration::from_millis(700));
        let wins = windows(&conn, root);
        for (name, x, y, w, h) in &wins {
            // FL's load report: missing samples / plugins. Ok sits at the
            // bottom, left of the help buttons.
            if name.starts_with("Problems loading") {
                click(&conn, root, x + 369, y + *h as i16 - 28);
            } else if name == "Error" {
                click(&conn, root, x + (*w as i16) / 2 - 10, y + *h as i16 - 22);
            }
        }
        // Done when FL has closed the file: the RIFF size is filled in last,
        // and the file stops growing (FL can hang on exit after a render).
        let finished = (|| {
            let mut file = std::fs::File::open(&out).ok()?;
            let mut header = [0; 12];
            file.read_exact(&mut header).ok()?;
            let size = file.metadata().ok()?.len();
            let riff_size = u32::from_le_bytes(header[4..8].try_into().ok()?) as u64;
            (size > 44
                && &header[..4] == b"RIFF"
                && &header[8..] == b"WAVE"
                && riff_size + 8 == size)
                .then_some(size)
        })();
        match (finished, gone_since) {
            (Some(len), Some((since, was))) if len == was => {
                if since.elapsed() > Duration::from_secs(2) {
                    break Ok(out.clone());
                }
            }
            (Some(len), _) => gone_since = Some((Instant::now(), len)),
            (None, _) => gone_since = None,
        }
        if start.elapsed() > timeout {
            break Err("FL Studio did not finish rendering in time".into());
        }
    };
    // Everything in the render prefix goes: FL lingers after a render.
    let _ = Command::new(bin.join("wineserver"))
        .env("WINEPREFIX", prefix)
        .arg("-k")
        .status();
    let _ = fl.kill();
    let _ = fl.wait();
    result
}

/// Render each of `channels` of the FL project `flp` alone and dry. The WAVs
/// are kept in the cache by the project's contents, so a second import is
/// instant. `say` hears progress.
pub fn freeze(
    flp: &[u8],
    name: &str,
    channels: &[(u32, String)],
    say: &dyn Fn(String),
) -> Result<Vec<(u32, PathBuf)>, String> {
    let hash = flp.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    let root = dirs::cache_dir()
        .ok_or("No cache folder")?
        .join("keyframexl/fl-render");
    let dir = root.join(format!("{name}-{hash:016x}-dry2"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let done = |ch: u32| dir.join(format!("channel {ch}.wav"));
    if channels.iter().all(|(c, _)| done(*c).is_file()) {
        return Ok(channels.iter().map(|(c, _)| (*c, done(*c))).collect());
    }
    let lock = std::fs::File::create(root.join("render.lock")).map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if channels.iter().all(|(c, _)| done(*c).is_file()) {
        return Ok(channels.iter().map(|(c, _)| (*c, done(*c))).collect());
    }
    say("Preparing FL Studio for rendering…".into());
    let (bin, prefix, exe) = crate::fl_link::render_setup()?;
    let display = Display::start()?;
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (k, (ch, label)) in channels.iter().enumerate() {
        if done(*ch).is_file() {
            out.push((*ch, done(*ch)));
            continue;
        }
        say(format!(
            "FL Studio renders {label} ({}/{})…",
            k + 1,
            channels.len()
        ));
        let solo = keyframexl::flp::solo_channel_flp(flp, *ch)?;
        let project = dir.join(format!("solo {ch}.flp"));
        std::fs::write(&project, solo).map_err(|e| e.to_string())?;
        let wav = render(
            &bin,
            &prefix,
            &exe,
            &display,
            &project,
            Duration::from_secs(900),
        );
        let _ = std::fs::remove_file(&project);
        let wav = match wav {
            Ok(wav) => wav,
            Err(e) => {
                eprintln!("FL Studio could not render {label}: {e}");
                errors.push(format!("{label}: {e}"));
                continue;
            }
        };
        std::fs::rename(&wav, done(*ch)).map_err(|e| e.to_string())?;
        out.push((*ch, done(*ch)));
    }
    if out.is_empty() && !errors.is_empty() {
        Err(errors.join("\n"))
    } else {
        Ok(out)
    }
}

/// `keyframexl flp-freeze project.flp <channel>...`: render channels and print the WAVs.
pub fn freeze_main(path: &Path, channels: &[u32]) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let list: Vec<(u32, String)> = channels
        .iter()
        .map(|c| (*c, format!("channel {c}")))
        .collect();
    for (c, wav) in freeze(&bytes, &name, &list, &|s| eprintln!("{s}"))? {
        println!("{c}\t{}", wav.display());
    }
    Ok(())
}
