//! Horizon entry point for the shared client, with no probe timeout.

use anyhow::{Context, Result};

const DIRECTORY: &str = "sdmc:/switch/vera20k";

unsafe extern "C" {
    fn platform_console_init();
    fn platform_applet_type() -> i32;
    fn platform_mount_user_save() -> u32;
    fn platform_commit_user_save() -> u32;
    fn platform_unmount_user_save();
    fn platform_commit_sdmc() -> u32;
    fn platform_romfs_init() -> bool;
    fn platform_romfs_exit();
}

fn run() -> Result<()> {
    unsafe { platform_console_init() };
    anyhow::ensure!(
        unsafe { platform_applet_type() } == 0,
        "the full client requires Application mode"
    );
    let save_result = unsafe { platform_mount_user_save() };
    anyhow::ensure!(
        save_result == 0,
        "mount user save data failed: {save_result:#010x}; launch the installed NSP and select a user"
    );
    let romfs_mounted = unsafe { platform_romfs_init() };
    let result = run_mounted(romfs_mounted);
    // App::run has returned and dropped its file handles before the final commit.
    let commit = commit_user_save();
    if romfs_mounted {
        unsafe { platform_romfs_exit() };
    }
    unsafe { platform_unmount_user_save() };
    result.and(commit)
}

fn run_mounted(romfs_mounted: bool) -> Result<()> {
    std::fs::create_dir_all(DIRECTORY).context("create client data directory")?;
    std::env::set_current_dir(DIRECTORY).context("enter client data directory")?;
    import_legacy_saves()?;
    let log_path =
        vera20k::util::logging::init_file_logger("client").context("initialize client log")?;
    vera20k::util::logging::install_panic_hook(Some(&log_path));
    log::info!("Starting shared VERA20k client on Horizon");
    log::info!("romfs_mounted={romfs_mounted}");
    vera20k::app::App::default().run()
}

fn commit_user_save() -> Result<()> {
    let result = unsafe { platform_commit_user_save() };
    anyhow::ensure!(result == 0, "commit user save data failed: {result:#010x}");
    Ok(())
}

fn import_legacy_saves() -> Result<()> {
    let legacy = std::path::Path::new("sdmc:/switch/vera20k/saves");
    let savedata = std::path::Path::new("vera20k-user:/saves");
    let marker = savedata.join(".legacy-import-complete");
    if marker
        .try_exists()
        .context("inspect legacy import marker")?
    {
        return Ok(());
    }
    if !legacy.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(savedata).context("create user save directory")?;
    let mut imported = false;
    for entry in std::fs::read_dir(legacy).context("read legacy SD save directory")? {
        let entry = entry.context("read legacy save entry")?;
        if !entry
            .file_type()
            .context("inspect legacy save entry")?
            .is_file()
        {
            continue;
        }
        if !matches!(
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("bin" | "sav")
        ) {
            continue;
        }
        let destination = savedata.join(entry.file_name());
        match std::fs::symlink_metadata(&destination) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect existing user save"),
        }
        // A partial copy must never look like a published save on the next run.
        let stage = savedata.join(".legacy-import-stage");
        match std::fs::remove_file(&stage) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("remove interrupted import stage"),
        }
        let mut source = std::fs::File::open(entry.path()).context("open legacy save")?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .context("create legacy import stage")?;
        let copied = std::io::copy(&mut source, &mut output).and_then(|_| output.sync_all());
        drop(output);
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&stage);
            return Err(error).context("copy legacy save into user data");
        }
        std::fs::rename(&stage, &destination).context("publish imported legacy save")?;
        commit_user_save()?;
        imported = true;
    }
    // Record completion per account so deleted saves do not reappear at launch.
    let marker_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .context("create legacy import marker")?;
    let synced = marker_file.sync_all();
    drop(marker_file);
    synced.context("sync legacy import marker")?;
    commit_user_save()?;
    if imported {
        log::info!("Imported legacy SD saves into the selected user's save data");
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    let status = match run() {
        Ok(()) => 0,
        Err(error) => {
            log::error!("Client exited with an error: {error:#}");
            let _ = std::fs::write(
                format!("{DIRECTORY}/client.last-error.txt"),
                format!("{error:#}\n"),
            );
            1
        }
    };
    log::logger().flush();
    unsafe { platform_commit_sdmc() };
    status
}
