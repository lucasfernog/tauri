#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

mod cmd;

use serde::Serialize;

#[derive(Serialize)]
struct Reply {
  data: String
}

struct DummyBackend {}
impl tauri_updater::updater::Backend for DummyBackend {
  fn is_uptodate(&self, version: String) -> Result<bool, String> {
    Ok(false)
  }
  fn update_url(&self, version: String) -> Result<String, String> {
    Ok("https://github.com/jaemk/self_update/releases/download/v9.9.10/self_update-v9.9.10-x86_64-unknown-linux-gnu.tar.gz".to_string())
  }
}

fn test_download() {
  let backend = DummyBackend {};
  let mut updater = tauri_updater::updater::UpdaterBuilder::new();
  updater = updater.current_version("1.3.0");
  updater = updater.bin_name("app.tar.gz");
  updater = updater.backend(backend);
  updater.build().unwrap().update().unwrap();
}

fn main() {
  test_download();
}
