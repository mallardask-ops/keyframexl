//! Wine / Proton / DXVK management for running Windows plugins through yabridge
//! on Linux. On non-Linux platforms every entry point is a no-op stub.

#[cfg(target_os = "linux")]
mod imp {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    /// The Wine `bin` folder plugin processes run Windows plugins with (yabridge
    /// finds `wine` through PATH). Set from the settings; None is the system Wine.
    static WINE_BIN: Mutex<Option<PathBuf>> = Mutex::new(None);

    /// The Wine `bin` folder plugins run with; `None` = the system Wine.
    pub fn wine_bin() -> Option<PathBuf> {
        WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_wine(bin: Option<PathBuf>) {
        let bin = bin.filter(|b| b.join("wine").exists());
        *WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()) = bin.clone();
        // Get the prefix ready now, so the first plugin does not wait for it.
        if let Some(bin) = bin {
            let _ = std::thread::Builder::new()
                .name("wine-prefix".into())
                .spawn(move || prepare_prefix(&bin));
        }
    }

    /// The prefix plugin processes use: $WINEPREFIX, else ~/.wine.
    pub fn wine_prefix() -> Option<PathBuf> {
        std::env::var_os("WINEPREFIX")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".wine")))
    }

    /// The wineserver the chosen Wine would run (system: the one on PATH).
    fn own_wineserver() -> Option<PathBuf> {
        let bin = WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let server = match bin {
            Some(b) => b.join("wineserver"),
            None => std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|d| d.join("wineserver"))
                .find(|p| p.is_file())?,
        };
        std::fs::canonicalize(&server).ok()
    }

    /// A wineserver from another Wine build holding our prefix: (pid, its program).
    /// Two Wine versions cannot share a prefix at once; while it runs, every
    /// plugin started with the chosen Wine fails ("Failed to start the Wine
    /// plugin host"), so the settings page offers to stop it.
    pub fn foreign_wineserver() -> Option<(u32, PathBuf)> {
        let ours = own_wineserver();
        wineservers_on_prefix()
            .into_iter()
            .find(|(_, exe)| ours.as_ref() != Some(exe))
    }

    /// Every wineserver holding our prefix: (pid, its program).
    fn wineservers_on_prefix() -> Vec<(u32, PathBuf)> {
        match wine_prefix() {
            Some(p) => wineservers_on(&p),
            None => Vec::new(),
        }
    }

    /// Wineservers serving `prefix`.
    fn wineservers_on(prefix: &Path) -> Vec<(u32, PathBuf)> {
        let mut out = Vec::new();
        let Ok(prefix) = std::fs::canonicalize(prefix) else {
            return out;
        };
        let home = dirs::home_dir();
        let Ok(procs) = std::fs::read_dir("/proc") else {
            return out;
        };
        for e in procs.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(exe) = std::fs::read_link(e.path().join("exe")) else {
                continue;
            };
            if exe.file_name().is_none_or(|n| n != "wineserver") {
                continue;
            }
            let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
            // Its prefix: WINEPREFIX from its environment, else ~/.wine.
            let env = std::fs::read(e.path().join("environ")).unwrap_or_default();
            let theirs = env
                .split(|b| *b == 0)
                .find_map(|kv| {
                    kv.strip_prefix(b"WINEPREFIX=")
                        .map(|v| PathBuf::from(String::from_utf8_lossy(v).to_string()))
                })
                .or_else(|| home.as_ref().map(|h| h.join(".wine")));
            let same_prefix = theirs
                .and_then(|p| std::fs::canonicalize(p).ok())
                .is_some_and(|p| p == prefix);
            if same_prefix {
                out.push((pid, exe));
            }
        }
        out
    }

    /// Proton's Wine always runs as "steamuser", so plugins under it look for
    /// their settings, licenses and library paths in users/steamuser, not in the
    /// user's own profile (SSD5: "Cannot find your instruments"). Make
    /// users/steamuser a link to the user's profile, keeping what was there as a
    /// backup folder. Only while no Wine runs on the prefix.
    fn link_steamuser(prefix: &Path) {
        let users = prefix.join("drive_c/users");
        let Some(me) = std::env::var_os("USER").filter(|u| !u.is_empty() && u != "steamuser")
        else {
            return;
        };
        let mine = users.join(&me);
        let steam = users.join("steamuser");
        if !mine.is_dir()
            || std::fs::symlink_metadata(&steam).is_ok_and(|m| m.file_type().is_symlink())
        {
            return;
        }
        if !wineservers_on(prefix).is_empty() {
            static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !TOLD.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "[wine] Wine runs on {}; steamuser is linked on a later start",
                    prefix.display()
                );
            }
            return;
        }
        if steam.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = users.join(format!("steamuser.before-keyframexl-{stamp}"));
            if let Err(e) = std::fs::rename(&steam, &backup) {
                eprintln!("[wine] cannot move {}: {e}", steam.display());
                return;
            }
            eprintln!("[wine] kept the old Proton profile as {}", backup.display());
        }
        match std::os::unix::fs::symlink(&me, &steam) {
            Ok(()) => eprintln!(
                "[wine] {} now points at {}",
                steam.display(),
                mine.display()
            ),
            Err(e) => eprintln!("[wine] cannot link steamuser: {e}"),
        }
    }

    /// Stop that wineserver the way its own Wine does (`wineserver -k`): every
    /// Windows program on the prefix closes. Then wait for it to go.
    pub fn stop_foreign_wineserver(pid: u32, exe: &Path) -> Result<(), String> {
        let prefix = wine_prefix().ok_or("No Wine prefix")?;
        std::process::Command::new(exe)
            .arg("-k")
            .env("WINEPREFIX", &prefix)
            .status()
            .map_err(|e| e.to_string())?;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while Path::new(&format!("/proc/{pid}")).exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if Path::new(&format!("/proc/{pid}")).exists() {
            Err("It is still running".into())
        } else {
            Ok(())
        }
    }

    /// One preparation at a time; plugin processes wait for it (`apply_wine`).
    static PREFIX_PREP: Mutex<()> = Mutex::new(());

    /// A Proton build finds its vkd3d DLLs only through placeholders in the
    /// prefix's system32, which `wineboot` writes when WINEDLLPATH names them.
    /// A prefix that this build updated without them (started bare, as yabridge
    /// does) lacks them, and wined3d → dxgi → the plugin fail to load. Update the
    /// prefix once with the right paths, as Proton's own launcher would.
    fn prepare_prefix(bin: &Path) {
        let _guard = PREFIX_PREP.lock().unwrap_or_else(|e| e.into_inner());
        let Some(dlls) = wine_dll_path(bin) else {
            return;
        };
        let Some(prefix) = wine_prefix() else { return };
        if let Some((pid, exe)) = foreign_wineserver() {
            // wineboot would fail the same way; the settings page offers to stop it.
            eprintln!(
                "[wine] {} holds {} (pid {pid}); not updating the prefix",
                exe.display(),
                prefix.display()
            );
            return;
        }
        link_steamuser(&prefix);
        update_prefix(bin, &prefix, dlls);
        if DXVK.load(std::sync::atomic::Ordering::Relaxed) && prefix.join("system.reg").exists() {
            install_dxvk(bin, &prefix);
        }
    }

    /// `wineboot -u` with the vkd3d paths when the placeholders are missing.
    fn update_prefix(bin: &Path, prefix: &Path, dlls: std::ffi::OsString) {
        // No prefix yet: Wine creates it with these paths on first use.
        if !prefix.join("system.reg").exists()
            || prefix
                .join("drive_c/windows/system32/libvkd3d-1.dll")
                .exists()
        {
            return;
        }
        eprintln!(
            "[wine] updating {} for {} (vkd3d placeholders)",
            prefix.display(),
            bin.display()
        );
        let mut path = vec![bin.to_path_buf()];
        if let Some(p) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&p));
        }
        let mut c = std::process::Command::new(bin.join("wine"));
        c.args(["wineboot", "-u"])
            .env("WINEPREFIX", prefix)
            .env("WINEDLLPATH", dlls)
            .env("WINEDEBUG", "-all")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Ok(joined) = std::env::join_paths(path) {
            c.env("PATH", joined);
        }
        match c.spawn() {
            Ok(mut child) => {
                let until = std::time::Instant::now() + std::time::Duration::from_secs(180);
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) if std::time::Instant::now() < until => {
                            std::thread::sleep(std::time::Duration::from_millis(200))
                        }
                        _ => {
                            let _ = child.kill();
                            break;
                        }
                    }
                }
                let ok = prefix
                    .join("drive_c/windows/system32/libvkd3d-1.dll")
                    .exists();
                eprintln!(
                    "[wine] prefix update {}",
                    if ok { "done" } else { "did not add vkd3d" }
                );
            }
            Err(e) => eprintln!("[wine] cannot run wineboot: {e}"),
        }
    }

    /// Put the chosen Wine first in a plugin process's PATH.
    pub fn apply_wine(c: &mut std::process::Command) {
        let Some(bin) = WINE_BIN.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return;
        };
        prepare_prefix(&bin);
        let mut path = vec![bin];
        if let Some(p) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(path) {
            c.env("PATH", joined);
        }
        if let Some(dlls) = wine_dll_path(&bin_dir(c)) {
            c.env("WINEDLLPATH", dlls);
        }
        if DXVK.load(std::sync::atomic::Ordering::Relaxed) && has_dxvk(&bin_dir(c)) {
            c.env("WINEDLLOVERRIDES", dxvk_overrides());
        }
    }

    /// Whether a Windows plugin is 32-bit: the DLL yabridge loads for a `.so`
    /// (beside it, usually a link into the prefix), read from its PE header.
    pub fn is_32bit_windows(path: &Path) -> bool {
        let dll = path.with_extension("dll");
        let Ok(d) = std::fs::read(&dll).or_else(|_| std::fs::read(path)) else {
            return false;
        };
        let pe = d
            .get(0x3c..0x40)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        match pe.and_then(|o| d.get(o..o + 6)) {
            Some(h) if &h[..4] == b"PE\0\0" => u16::from_le_bytes([h[4], h[5]]) == 0x14c,
            _ => false,
        }
    }

    /// A Wine build that still runs 32-bit Windows programs the old way
    /// (yabridge's 32-bit host is a 32-bit winelib program): Proton's.
    fn wine32_bin() -> Option<PathBuf> {
        let builds = wine_builds();
        let pick = |w: &str| {
            builds
                .iter()
                .filter_map(|(n, b)| b.clone().filter(|_| n.contains(w)))
                .next()
        };
        pick("Proton-CachyOS")
            .or_else(|| pick("Proton"))
            .or_else(|| pick("GE"))
    }

    /// The prefix 32-bit plugins run in when the rest use the system Wine: two
    /// Wine versions cannot share one prefix. A copy of the plugin prefix
    /// (reflinked where the filesystem can, so it costs no space), made once.
    fn wine32_prefix() -> Option<PathBuf> {
        static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
        let mut ready = READY.lock().unwrap_or_else(|e| e.into_inner());
        if ready.is_some() {
            return ready.clone();
        }
        let dir = dirs::data_dir()?.join("keyframexl/wine32-prefix");
        if !dir.join("system.reg").exists() {
            let src = wine_prefix()?;
            let _ = std::fs::create_dir_all(dir.parent()?);
            let _ = std::fs::remove_dir_all(&dir);
            let ok = std::process::Command::new("cp")
                .args(["-a", "--reflink=always"])
                .arg(&src)
                .arg(&dir)
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                // No reflinks here: a fresh prefix with the plugin prefix's
                // registry (plugins load from their own folders by path).
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).ok()?;
                for f in ["system.reg", "user.reg", "userdef.reg"] {
                    let _ = std::fs::copy(src.join(f), dir.join(f));
                }
            }
            eprintln!(
                "[wine] 32-bit plugin prefix at {} ({})",
                dir.display(),
                if ok { "reflinked copy" } else { "fresh" }
            );
        }
        *ready = Some(dir.clone());
        Some(dir)
    }

    /// `apply_wine` for one plugin: a 32-bit Windows plugin under the system
    /// Wine (whose new WoW64 cannot start yabridge's 32-bit host: c000007b) runs
    /// with Proton in its own prefix instead.
    pub fn apply_wine_for(c: &mut std::process::Command, plugin: &Path) {
        if wine_bin().is_none()
            && is_32bit_windows(plugin)
            && let Some(bin) = wine32_bin()
            && let Some(prefix) = wine32_prefix()
        {
            let mut path = vec![bin.clone()];
            if let Some(p) = std::env::var_os("PATH") {
                path.extend(std::env::split_paths(&p));
            }
            if let Ok(joined) = std::env::join_paths(path) {
                c.env("PATH", joined);
            }
            if let Some(dlls) = wine_dll_path(&bin) {
                {
                    let _guard = PREFIX_PREP.lock().unwrap_or_else(|e| e.into_inner());
                    update_prefix(&bin, &prefix, dlls.clone());
                }
                c.env("WINEDLLPATH", dlls);
            }
            c.env("WINEPREFIX", &prefix);
            return;
        }
        apply_wine(c);
    }

    /// Use the build's DXVK for Direct3D in plugin processes (Settings → Plugins).
    static DXVK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

    pub fn set_dxvk(on: bool) {
        DXVK.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether a Wine bin folder comes with DXVK (Proton builds do).
    pub fn has_dxvk(bin: &Path) -> bool {
        dxvk_dir(bin).is_some()
    }

    fn dxvk_dir(bin: &Path) -> Option<PathBuf> {
        let root = bin.parent()?;
        ["lib/wine/dxvk", "lib64/wine/dxvk"]
            .iter()
            .map(|d| root.join(d))
            .find(|d| d.join("x86_64-windows/dxgi.dll").is_file())
    }

    /// JUCE 8 plugins (SSD5 and others) draw with Direct2D and repaint on
    /// `IDXGIOutput::WaitForVBlank`, a stub in Wine's dxgi: they draw once and
    /// then never again, so clicks seem to do nothing. DXVK implements it.
    fn dxvk_overrides() -> std::ffi::OsString {
        let mut overrides = "dxgi,d3d11,d3d10core=n,b".to_string();
        if let Some(old) = std::env::var_os("WINEDLLOVERRIDES").filter(|v| !v.is_empty()) {
            overrides = format!("{};{overrides}", old.to_string_lossy());
        }
        overrides.into()
    }

    /// Copy the build's DXVK DLLs into the prefix (system32: 64-bit, syswow64:
    /// 32-bit) when they differ. Each file is replaced by a rename, so a process
    /// that has the old one mapped keeps working.
    fn install_dxvk(bin: &Path, prefix: &Path) {
        let Some(dir) = dxvk_dir(bin) else { return };
        for (arch, sys) in [("x86_64-windows", "system32"), ("i386-windows", "syswow64")] {
            let to_dir = prefix.join("drive_c/windows").join(sys);
            if !to_dir.is_dir() {
                continue;
            }
            for name in ["dxgi.dll", "d3d11.dll", "d3d10core.dll"] {
                let from = dir.join(arch).join(name);
                let to = to_dir.join(name);
                let Ok(new) = std::fs::read(&from) else {
                    continue;
                };
                if std::fs::read(&to).is_ok_and(|old| old == new) {
                    continue;
                }
                let tmp = to_dir.join(format!(".{name}.keyframexl"));
                match std::fs::write(&tmp, &new).and_then(|_| std::fs::rename(&tmp, &to)) {
                    Ok(()) => eprintln!("[wine] DXVK {name} → {}", to.display()),
                    Err(e) => eprintln!("[wine] cannot install DXVK {name}: {e}"),
                }
            }
        }
    }

    /// The chosen bin folder (first PATH entry `apply_wine` set).
    fn bin_dir(c: &std::process::Command) -> PathBuf {
        c.get_envs()
            .find(|(k, _)| *k == "PATH")
            .and_then(|(_, v)| v)
            .and_then(|v| std::env::split_paths(v).next())
            .unwrap_or_default()
    }

    /// Proton keeps vkd3d apart (`files/lib/vkd3d`), and its launcher puts it on
    /// WINEDLLPATH; started bare, wined3d → dxgi fail and plugins such as SSD5
    /// do not load. Same folders as Proton's own launcher, plus any WINEDLLPATH set.
    fn wine_dll_path(bin: &Path) -> Option<std::ffi::OsString> {
        let root = bin.parent()?;
        let lib = ["lib", "lib64"]
            .iter()
            .map(|l| root.join(l))
            .find(|l| l.join("vkd3d").is_dir())?;
        let mut dirs = vec![lib.join("vkd3d"), lib.join("wine")];
        if let Some(p) = std::env::var_os("WINEDLLPATH") {
            dirs.extend(std::env::split_paths(&p));
        }
        std::env::join_paths(dirs).ok()
    }

    /// Wine builds on this computer: (label, bin folder). The system one first
    /// (bin None). Looks in /opt, PortProton, Steam compatibility tools and Lutris.
    pub fn wine_builds() -> Vec<(String, Option<PathBuf>)> {
        let mut out = vec![("System Wine".to_string(), None)];
        // ~/.steam/root links to the same Steam folder: compare real paths.
        let mut seen = std::collections::HashSet::new();
        let mut add = |label: String, bin: PathBuf| {
            if bin.join("wine").is_file()
                && seen.insert(std::fs::canonicalize(&bin).unwrap_or_else(|_| bin.clone()))
            {
                out.push((label, Some(bin)));
            }
        };
        let list = |d: &Path| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(d)
                .map(|r| r.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            v.sort();
            v
        };
        for d in list(Path::new("/opt")).into_iter().filter(|d| {
            d.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("wine"))
        }) {
            add(
                d.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                d.join("bin"),
            );
        }
        if let Some(h) = dirs::home_dir() {
            for (root, kind) in [
                (h.join("PortProton/data/dist"), "PortProton"),
                (h.join(".local/share/Steam/compatibilitytools.d"), "Steam"),
                (h.join(".steam/root/compatibilitytools.d"), "Steam"),
                (h.join(".local/share/lutris/runners/wine"), "Lutris"),
            ] {
                for d in list(&root) {
                    let name = d
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    for bin in [d.join("bin"), d.join("files/bin"), d.join("dist/bin")] {
                        add(format!("{name} ({kind})"), bin);
                    }
                }
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn wine_builds_start_with_the_system_one_and_point_at_real_wine() {
            let builds = wine_builds();
            assert_eq!(builds[0], ("System Wine".to_string(), None));
            for (label, bin) in &builds[1..] {
                let bin = bin.as_ref().unwrap();
                assert!(bin.join("wine").is_file(), "{label}: {}", bin.display());
                println!("{label}: {}", bin.display());
            }
        }

        #[test]
        fn another_wines_server_on_the_prefix_is_found_and_stopped() {
            // Needs real Wine builds: KEYFRAMEXL_TEST_WINE=<a non-system wine bin folder>.
            let Some(other) = std::env::var_os("KEYFRAMEXL_TEST_WINE").map(PathBuf::from) else {
                return;
            };
            let dir = tempfile::tempdir().unwrap();
            // SAFETY: test-only; no other test reads WINEPREFIX.
            unsafe { std::env::set_var("WINEPREFIX", dir.path()) };
            std::fs::create_dir_all(dir.path()).unwrap();
            set_wine(Some(other));
            assert!(foreign_wineserver().is_none(), "nothing runs yet");
            std::process::Command::new("wineserver")
                .arg("-p20")
                .env("WINEPREFIX", dir.path())
                .status()
                .unwrap();
            let (pid, exe) = (0..50)
                .find_map(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    foreign_wineserver()
                })
                .expect("the system wineserver is foreign to the chosen build");
            assert!(exe.ends_with("wineserver"));
            stop_foreign_wineserver(pid, &exe).unwrap();
            assert!(foreign_wineserver().is_none());
            set_wine(None);
        }

        #[test]
        fn protons_steamuser_profile_becomes_the_users_own() {
            let Some(me) = std::env::var_os("USER").filter(|u| !u.is_empty() && u != "steamuser")
            else {
                return;
            };
            let dir = tempfile::tempdir().unwrap();
            let users = dir.path().join("drive_c/users");
            std::fs::create_dir_all(users.join(&me).join("AppData/Roaming/ssd_sampler5")).unwrap();
            std::fs::write(
                users
                    .join(&me)
                    .join("AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
                "base dir",
            )
            .unwrap();
            std::fs::create_dir_all(users.join("steamuser/AppData/Roaming/ssd_sampler5")).unwrap();
            std::fs::write(
                users.join("steamuser/AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
                "empty",
            )
            .unwrap();
            link_steamuser(dir.path());
            let seen = std::fs::read_to_string(
                users.join("steamuser/AppData/Roaming/ssd_sampler5/ssd_sampler5.ini"),
            )
            .unwrap();
            assert_eq!(seen, "base dir", "Proton now reads the user's own settings");
            let backups: Vec<_> = std::fs::read_dir(&users)
                .unwrap()
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("steamuser.before-keyframexl-")
                })
                .collect();
            assert_eq!(backups.len(), 1, "the old profile is kept");
            link_steamuser(dir.path());
            assert!(
                std::fs::symlink_metadata(users.join("steamuser"))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }

        #[test]
        fn dxvk_goes_into_the_prefix_by_rename_and_only_when_it_differs() {
            let dir = tempfile::tempdir().unwrap();
            let files = dir.path().join("files");
            std::fs::create_dir_all(files.join("bin")).unwrap();
            for arch in ["x86_64-windows", "i386-windows"] {
                let d = files.join("lib/wine/dxvk").join(arch);
                std::fs::create_dir_all(&d).unwrap();
                for n in ["dxgi.dll", "d3d11.dll", "d3d10core.dll"] {
                    std::fs::write(d.join(n), format!("dxvk {arch} {n}")).unwrap();
                }
            }
            let bin = files.join("bin");
            assert!(has_dxvk(&bin));
            let prefix = dir.path().join("pfx");
            let sys = prefix.join("drive_c/windows/system32");
            std::fs::create_dir_all(&sys).unwrap();
            std::fs::create_dir_all(prefix.join("drive_c/windows/syswow64")).unwrap();
            std::fs::write(sys.join("dxgi.dll"), "wine placeholder").unwrap();
            install_dxvk(&bin, &prefix);
            assert_eq!(
                std::fs::read_to_string(sys.join("dxgi.dll")).unwrap(),
                "dxvk x86_64-windows dxgi.dll"
            );
            assert_eq!(
                std::fs::read_to_string(prefix.join("drive_c/windows/syswow64/d3d11.dll")).unwrap(),
                "dxvk i386-windows d3d11.dll"
            );
            use std::os::unix::fs::MetadataExt;
            let ino = std::fs::metadata(sys.join("dxgi.dll")).unwrap().ino();
            install_dxvk(&bin, &prefix);
            assert_eq!(std::fs::metadata(sys.join("dxgi.dll")).unwrap().ino(), ino);
            assert!(!has_dxvk(&dir.path().join("plain/bin")));
        }

        #[test]
        fn proton_builds_get_their_vkd3d_on_the_dll_path() {
            let dir = tempfile::tempdir().unwrap();
            let files = dir.path().join("files");
            std::fs::create_dir_all(files.join("bin")).unwrap();
            std::fs::create_dir_all(files.join("lib/vkd3d/x86_64-windows")).unwrap();
            std::fs::create_dir_all(files.join("lib/wine")).unwrap();
            let got = wine_dll_path(&files.join("bin")).expect("a Proton layout");
            let dirs: Vec<_> = std::env::split_paths(&got).collect();
            assert_eq!(dirs[0], files.join("lib/vkd3d"));
            assert_eq!(dirs[1], files.join("lib/wine"));
            let plain = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(plain.path().join("bin")).unwrap();
            assert!(wine_dll_path(&plain.path().join("bin")).is_none());
        }

        #[test]
        fn the_chosen_wine_goes_first_in_path() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("wine"), b"").unwrap();
            set_wine(Some(dir.path().to_path_buf()));
            let mut c = std::process::Command::new("true");
            apply_wine(&mut c);
            let path = c
                .get_envs()
                .find(|(k, _)| *k == "PATH")
                .and_then(|(_, v)| v)
                .unwrap()
                .to_owned();
            assert!(std::env::split_paths(&path).next() == Some(dir.path().to_path_buf()));
            set_wine(None);
        }
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn set_wine(_bin: Option<PathBuf>) {}
    pub fn set_dxvk(_on: bool) {}
    pub fn wine_prefix() -> Option<PathBuf> {
        None
    }
    pub fn foreign_wineserver() -> Option<(u32, PathBuf)> {
        None
    }
    pub fn stop_foreign_wineserver(_pid: u32, _exe: &Path) -> Result<(), String> {
        Ok(())
    }
    pub fn has_dxvk(_bin: &Path) -> bool {
        false
    }
    pub fn wine_builds() -> Vec<(String, Option<PathBuf>)> {
        Vec::new()
    }
    #[allow(dead_code)]
    pub fn apply_wine(_c: &mut std::process::Command) {}
    pub fn apply_wine_for(_c: &mut std::process::Command, _plugin: &Path) {}
}

pub use imp::*;
