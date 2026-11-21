//! MP3 export through a statically compiled LAME (`mp3lame-sys`), so MP3
//! export works out of the box on Linux and Windows with no external library.
//! The ID3v2.4 tag (UTF-8, so any script) is written here, not by LAME,
//! whose own tag calls take Latin-1 only. LAME's info frame (delay and
//! padding) leads the audio, so players play exactly the rendered length.

use crate::sequence::ProjectInfo;
use std::ffi::c_int;
use std::io::Write;
use std::path::Path;

/// LAME is statically linked into the binary, so MP3 export is always available.
pub fn available() -> bool {
    true
}

/// Encode stereo `frames` at `rate` as constant-bitrate MP3 (`kbps`), with
/// `info` as the tag.
pub fn encode(
    frames: &[[f32; 2]],
    rate: u32,
    kbps: u16,
    info: &ProjectInfo,
) -> Result<Vec<u8>, String> {
    let mut out = id3(info);
    let head = out.len();
    // SAFETY: standard libmp3lame C API lifecycle on a freshly initialised handle.
    unsafe {
        let gf = mp3lame_sys::lame_init();
        if gf.is_null() {
            return Err("LAME did not start".into());
        }
        let ok = mp3lame_sys::lame_set_in_samplerate(gf, rate as c_int) >= 0
            && mp3lame_sys::lame_set_num_channels(gf, 2) >= 0
            && mp3lame_sys::lame_set_brate(gf, kbps as c_int) >= 0
            && mp3lame_sys::lame_set_quality(gf, 2) >= 0
            && mp3lame_sys::lame_set_bWriteVbrTag(gf, 1) >= 0
            && mp3lame_sys::lame_init_params(gf) >= 0;
        if !ok {
            mp3lame_sys::lame_close(gf);
            return Err(format!("LAME refused {kbps} kbps at {rate} Hz"));
        }
        const CHUNK: usize = 8192;
        let mut buf = vec![0u8; CHUNK * 5 / 4 + 7200];
        for part in frames.chunks(CHUNK) {
            let n = mp3lame_sys::lame_encode_buffer_interleaved_ieee_float(
                gf,
                part.as_ptr() as *const f32,
                part.len() as c_int,
                buf.as_mut_ptr(),
                buf.len() as c_int,
            );
            if n < 0 {
                mp3lame_sys::lame_close(gf);
                return Err(format!("LAME failed ({n})"));
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        let n = mp3lame_sys::lame_encode_flush(gf, buf.as_mut_ptr(), buf.len() as c_int);
        if n < 0 {
            mp3lame_sys::lame_close(gf);
            return Err(format!("LAME failed to finish ({n})"));
        }
        out.extend_from_slice(&buf[..n as usize]);
        // The first frame becomes LAME's info frame: encoder delay and
        // padding, so players trim the silence and loops stay gapless.
        let t = mp3lame_sys::lame_get_lametag_frame(gf, buf.as_mut_ptr(), buf.len());
        if t > 0 && t <= buf.len() && head + t <= out.len() {
            out[head..head + t].copy_from_slice(&buf[..t]);
        }
        mp3lame_sys::lame_close(gf);
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
