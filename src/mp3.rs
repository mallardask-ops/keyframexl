//! MP3 export through the system's LAME (`libmp3lame.so.0`), loaded when
//! first used: no build dependency, and a clear message when it is missing.
//! The ID3v2.4 tag (UTF-8, so any script) is written here, not by LAME,
//! whose own tag calls take Latin-1 only. LAME's info frame (delay and
//! padding) leads the audio, so players play exactly the rendered length.

use crate::sequence::ProjectInfo;
#[cfg(unix)]
use std::ffi::{CStr, c_char};
use std::ffi::{c_int, c_uchar, c_void};
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

type Lame = *mut c_void;

struct Api {
    init: unsafe extern "C" fn() -> Lame,
    set_in_samplerate: unsafe extern "C" fn(Lame, c_int) -> c_int,
    set_num_channels: unsafe extern "C" fn(Lame, c_int) -> c_int,
    set_brate: unsafe extern "C" fn(Lame, c_int) -> c_int,
    set_quality: unsafe extern "C" fn(Lame, c_int) -> c_int,
    set_write_vbr_tag: unsafe extern "C" fn(Lame, c_int) -> c_int,
    init_params: unsafe extern "C" fn(Lame) -> c_int,
    encode: unsafe extern "C" fn(Lame, *const f32, c_int, *mut c_uchar, c_int) -> c_int,
    flush: unsafe extern "C" fn(Lame, *mut c_uchar, c_int) -> c_int,
    close: unsafe extern "C" fn(Lame) -> c_int,
    lametag: unsafe extern "C" fn(Lame, *mut c_uchar, usize) -> usize,
}

// The library handle stays open for the program's life; the functions are
// plain C calls on a handle each encode owns.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(load).as_ref().map_err(Clone::clone)
}

#[cfg(unix)]
fn load() -> Result<Api, String> {
    let names: [&CStr; 2] = [c"libmp3lame.so.0", c"libmp3lame.so"];
    let lib = names
        .iter()
        .map(|n| unsafe { libc::dlopen(n.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) })
        .find(|h| !h.is_null());
    let Some(lib) = lib else {
        return Err("MP3 needs LAME (libmp3lame); install the lame package".into());
    };
    let sym = |name: &CStr| -> Result<*mut c_void, String> {
        let p = unsafe { libc::dlsym(lib, name.as_ptr() as *const c_char) };
        if p.is_null() {
            Err(format!("LAME has no {}", name.to_string_lossy()))
        } else {
            Ok(p)
        }
    };
    // SAFETY: each symbol is LAME's public C function with this signature (lame.h).
    unsafe {
        Ok(Api {
            init: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> Lame>(sym(
                c"lame_init",
            )?),
            set_in_samplerate: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, c_int) -> c_int,
            >(sym(c"lame_set_in_samplerate")?),
            set_num_channels: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, c_int) -> c_int,
            >(sym(c"lame_set_num_channels")?),
            set_brate: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Lame, c_int) -> c_int>(
                sym(c"lame_set_brate")?,
            ),
            set_quality: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, c_int) -> c_int,
            >(sym(c"lame_set_quality")?),
            set_write_vbr_tag: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, c_int) -> c_int,
            >(sym(c"lame_set_bWriteVbrTag")?),
            init_params: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Lame) -> c_int>(
                sym(c"lame_init_params")?,
            ),
            encode: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, *const f32, c_int, *mut c_uchar, c_int) -> c_int,
            >(sym(c"lame_encode_buffer_interleaved_ieee_float")?),
            flush: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, *mut c_uchar, c_int) -> c_int,
            >(sym(c"lame_encode_flush")?),
            close: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Lame) -> c_int>(sym(
                c"lame_close",
            )?),
            lametag: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(Lame, *mut c_uchar, usize) -> usize,
            >(sym(c"lame_get_lametag_frame")?),
        })
    }
}

#[cfg(not(unix))]
fn load() -> Result<Api, String> {
    Err("MP3 export requires static LAME (scheduled for W1)".into())
}

/// LAME can be loaded (the export dialog greys MP3 out otherwise).
pub fn available() -> bool {
    api().is_ok()
}

/// Encode stereo `frames` at `rate` as constant-bitrate MP3 (`kbps`), with
/// `info` as the tag.
pub fn encode(
    frames: &[[f32; 2]],
    rate: u32,
    kbps: u16,
    info: &ProjectInfo,
) -> Result<Vec<u8>, String> {
    let api = api()?;
    let mut out = id3(info);
    let head = out.len();
    unsafe {
        let gf = (api.init)();
        if gf.is_null() {
            return Err("LAME did not start".into());
        }
        let ok = (api.set_in_samplerate)(gf, rate as c_int) >= 0
            && (api.set_num_channels)(gf, 2) >= 0
            && (api.set_brate)(gf, kbps as c_int) >= 0
            && (api.set_quality)(gf, 2) >= 0
            && (api.set_write_vbr_tag)(gf, 1) >= 0
            && (api.init_params)(gf) >= 0;
        if !ok {
            (api.close)(gf);
            return Err(format!("LAME refused {kbps} kbps at {rate} Hz"));
        }
        const CHUNK: usize = 8192;
        let mut buf = vec![0u8; CHUNK * 5 / 4 + 7200];
        for part in frames.chunks(CHUNK) {
            let n = (api.encode)(
                gf,
                part.as_ptr() as *const f32,
                part.len() as c_int,
                buf.as_mut_ptr(),
                buf.len() as c_int,
            );
            if n < 0 {
                (api.close)(gf);
                return Err(format!("LAME failed ({n})"));
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        let n = (api.flush)(gf, buf.as_mut_ptr(), buf.len() as c_int);
        if n < 0 {
            (api.close)(gf);
            return Err(format!("LAME failed to finish ({n})"));
        }
        out.extend_from_slice(&buf[..n as usize]);
        // The first frame becomes LAME's info frame: encoder delay and
        // padding, so players trim the silence and loops stay gapless.
        let t = (api.lametag)(gf, buf.as_mut_ptr(), buf.len());
        if t > 0 && t <= buf.len() && head + t <= out.len() {
            out[head..head + t].copy_from_slice(&buf[..t]);
        }
        (api.close)(gf);
    }
    Ok(out)
}

pub fn write(
    path: &Path,
    frames: &[[f32; 2]],
    rate: u32,
    kbps: u16,
    info: &ProjectInfo,
) -> Result<(), String> {
    let bytes = encode(frames, rate, kbps, info)?;
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())
}

fn syncsafe(n: usize) -> [u8; 4] {
    [
        (n >> 21 & 0x7f) as u8,
        (n >> 14 & 0x7f) as u8,
        (n >> 7 & 0x7f) as u8,
        (n & 0x7f) as u8,
    ]
}

/// An ID3v2.4 tag with the project's metadata in UTF-8, plus the encoder;
/// empty fields are left out.
pub fn id3(info: &ProjectInfo) -> Vec<u8> {
    let mut frames = Vec::new();
    let mut frame = |id: &[u8; 4], body: Vec<u8>| {
        frames.extend_from_slice(id);
        frames.extend_from_slice(&syncsafe(body.len()));
        frames.extend_from_slice(&[0, 0]);
        frames.extend_from_slice(&body);
    };
    let text = |s: &str| std::iter::once(3u8).chain(s.bytes()).collect::<Vec<u8>>();
    for (id, s) in [
        (b"TIT2", &info.title),
        (b"TPE1", &info.author),
        (b"TALB", &info.album),
        (b"TCON", &info.genre),
        (b"TDRC", &info.year),
        (b"TCOP", &info.copyright),
    ] {
        if !s.trim().is_empty() {
            frame(id, text(s.trim()));
        }
    }
    if !info.url.trim().is_empty() {
        // WXXX: text encoding (UTF-8 = 3), empty description (0-terminated), URL bytes.
        let mut body = vec![3u8, 0];
        body.extend_from_slice(info.url.trim().as_bytes());
        frame(b"WXXX", body);
    }
    if !info.comments.trim().is_empty() {
        // Encoding, language, empty description (terminated), text.
        let mut body = vec![3u8, b'e', b'n', b'g', 0];
        body.extend_from_slice(info.comments.trim().as_bytes());
        frame(b"COMM", body);
    }
    frame(
        b"TSSE",
        text(&format!("KeyFrameXL {} + LAME", crate::version::VERSION)),
    );
    let mut tag = b"ID3\x04\x00\x00".to_vec();
    tag.extend_from_slice(&syncsafe(frames.len()));
    tag.extend_from_slice(&frames);
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_carries_the_project_info_in_utf8() {
        let info = ProjectInfo {
            title: "Нічне місто".into(),
            author: "Me".into(),
            album: "Midnight EP".into(),
            genre: String::new(),
            year: "2026".into(),
            copyright: "CC0".into(),
            url: "https://example.com/track".into(),
            comments: "808 з Kit 3".into(),
            ..Default::default()
        };
        let tag = id3(&info);
        assert_eq!(&tag[..5], b"ID3\x04\x00");
        let size = tag[6..10].iter().fold(0usize, |n, b| n << 7 | *b as usize);
        assert_eq!(size + 10, tag.len());
        let has = |needle: &[u8]| tag.windows(needle.len()).any(|w| w == needle);
        assert!(
            has("Нічне місто".as_bytes())
                && has(b"TIT2")
                && has(b"TPE1")
                && has(b"TALB")
                && has(b"TDRC")
                && has(b"TCOP")
                && has(b"WXXX")
                && has(b"COMM")
        );
        assert!(has(b"https://example.com/track"));
        assert!(!has(b"TCON"), "an empty genre is left out");
    }

    #[test]
    fn a_tone_encodes_to_mp3_frames_when_lame_is_installed() {
        if !available() {
            return;
        }
        let rate = 48_000;
        let frames: Vec<[f32; 2]> = (0..rate)
            .map(|i| [(i as f32 * 0.05).sin() * 0.5; 2])
            .collect();
        let bytes = encode(&frames, rate, 192, &ProjectInfo::default()).unwrap();
        let tag = 10
            + bytes[6..10]
                .iter()
                .fold(0usize, |n, b| n << 7 | *b as usize);
        // An MPEG-1 Layer III frame right after the tag: LAME's info frame.
        assert_eq!(bytes[tag], 0xff);
        assert_eq!(bytes[tag + 1] & 0xfe, 0xfa);
        assert!(
            bytes[tag..tag + 200]
                .windows(4)
                .any(|w| w == b"Info" || w == b"Xing"),
            "gapless info frame"
        );
        // One second at 192 kbps is about 24 kB.
        let audio = bytes.len() - tag;
        assert!((20_000..30_000).contains(&audio), "{audio} bytes");
    }
}
