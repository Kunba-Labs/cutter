//! In-app updates, membox's shape.
//!
//! latest.json sits on the newest GitHub release (Kunba-Labs/cutter) and is signed with a key whose
//! public half is baked into the binary (tauri.release.conf.json). That signature is the security
//! model: GitHub can serve any bytes it likes and the client refuses anything the private key
//! (~/Desktop/cuttar-updater-key) did not sign.
//!
//! Only a build with the release config carries `plugins.updater` (CI, `yarn install:app`,
//! `bin/app-build`). Dev builds have none, and the plugin refuses to initialise without it, so
//! main.rs registers it on the same test as `supported()`.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

pub fn supported(app: &AppHandle) -> bool {
    app.config().plugins.0.contains_key("updater")
}

/// Look shortly after launch, then every half hour while the app is open. A hit emits
/// `update://available` with the version; the status bar offers the restart.
pub fn poll(app: &AppHandle) {
    if !supported(app) {
        return;
    }
    let app = app.clone();
    // A plain thread for the wait: sleeping on a runtime worker parks a thread the plugins share.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        loop {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                match check(&app).await {
                    Ok(Some(v)) => {
                        log::info!("update available: {v}");
                        let _ = app.emit("update://available", v);
                    }
                    Ok(None) => {}
                    // Offline, or no release yet. Not worth a word.
                    Err(e) => log::debug!("update check failed: {e}"),
                }
            });
            std::thread::sleep(std::time::Duration::from_secs(30 * 60));
        }
    });
}

/// The plugin's default client waits forever; a deadline turns a stalled download into an error.
fn updater(app: &AppHandle, secs: u64) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder().timeout(std::time::Duration::from_secs(secs)).build().map_err(|e| e.to_string())
}

async fn check(app: &AppHandle) -> Result<Option<String>, String> {
    // Without the plugin `updater_builder()` reaches for unmanaged state and panics.
    if !supported(app) {
        return Err("this build has no updater (a dev build)".into());
    }
    let found = updater(app, 30)?.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|u| u.version))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<Option<String>, String> {
    check(&app).await
}

static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Download, verify, replace the .app, relaunch. The workers are held for the whole of it, so no
/// job starts that the restart would cut off; one already running refuses the update. A failed
/// update releases the hold. One install at a time.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    if !supported(&app) {
        return Err("this build has no updater".into());
    }
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("The update is already downloading.".into());
    }
    let lib = app.state::<crate::Core>().0.clone();
    if lib.hold_jobs(true) > 0 {
        lib.hold_jobs(false);
        INSTALLING.store(false, Ordering::SeqCst);
        return Err("Let the running jobs finish first, then restart to update.".into());
    }
    let res: Result<(), String> = async {
        let update = updater(&app, 10 * 60)?.check().await.map_err(|e| e.to_string())?.ok_or_else(|| "no update available".to_string())?;
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())
    }
    .await;
    match res {
        // The hold is not stored anywhere: after the relaunch the queue carries on.
        Ok(()) => app.restart(),
        Err(e) => {
            lib.hold_jobs(false);
            INSTALLING.store(false, Ordering::SeqCst);
            Err(e)
        }
    }
}
