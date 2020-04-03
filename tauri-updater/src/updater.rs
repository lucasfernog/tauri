use std::boxed::Box;
use std::env;
use std::path::PathBuf;

use crate::http;

use tauri_api::file::{Extract, Move};

mod backend;
pub use backend::Backend;

/// Status returned after updating
///
/// Wrapped `String`s are version tags
#[derive(Debug, Clone)]
pub enum Status {
  UpToDate(String),
  Updated(String),
}
impl Status {
  /// Return the version tag
  pub fn version(&self) -> &str {
    use Status::*;
    match *self {
      UpToDate(ref s) => s,
      Updated(ref s) => s,
    }
  }

  /// Returns `true` if `Status::UpToDate`
  pub fn uptodate(&self) -> bool {
    match *self {
      Status::UpToDate(_) => true,
      _ => false,
    }
  }

  /// Returns `true` if `Status::Updated`
  pub fn updated(&self) -> bool {
    match *self {
      Status::Updated(_) => true,
      _ => false,
    }
  }
}

#[derive(Clone, Debug)]
pub struct Release {
  pub version: String,
  pub asset_name: String,
  pub download_url: String,
}

#[derive(Default)]
pub struct UpdaterBuilder {
  bin_name: Option<String>,
  current_version: Option<String>,
  on_progress: Option<Box<dyn Fn(f64)>>,
  backend: Option<Box<dyn Backend>>,
}

impl UpdaterBuilder {
  /// Initialize a new builder
  pub fn new() -> UpdaterBuilder {
    Default::default()
  }

  /// Set the current app version, used to compare against the latest available version.
  /// The `cargo_crate_version!` macro can be used to pull the version from your `Cargo.toml`
  pub fn current_version(mut self, ver: &str) -> Self {
    self.current_version = Some(ver.to_owned());
    self
  }

  /// Set the exe's name.
  pub fn bin_name(mut self, name: &str) -> Self {
    self.bin_name = Some(name.to_owned());
    self
  }

  pub fn on_progress<F: Fn(f64) + 'static>(mut self, handler: F) -> Self {
    self.on_progress = Some(Box::new(handler));
    self
  }

  pub fn backend(mut self, backend: impl Backend + 'static) -> Self {
    self.backend = Some(Box::new(backend));
    self
  }

  /// Confirm config and create a ready-to-use `Updater`
  ///
  /// * Errors:
  ///     * Config - Invalid `Update` configuration
  pub fn build(self) -> crate::Result<Updater> {
    Ok(Updater {
      bin_name: if let Some(ref name) = self.bin_name {
        name.to_owned()
      } else {
        bail!(crate::ErrorKind::Config, "`bin_name` required")
      },
      current_version: if let Some(ref ver) = self.current_version {
        ver.to_owned()
      } else {
        bail!(crate::ErrorKind::Config, "`current_version` required")
      },
      on_progress: self.on_progress,
      backend: if let Some(backend) = self.backend {
        backend
      } else {
        bail!(crate::ErrorKind::Config, "`backend` required")
      },
    })
  }
}

/// Updates to a specified or latest release distributed
pub struct Updater {
  bin_name: String,
  current_version: String,
  on_progress: Option<Box<dyn Fn(f64)>>,
  backend: Box<dyn Backend>,
}

impl Updater {
  fn print_flush(&self, msg: &str) -> crate::Result<()> {
    if cfg!(debug_assertions) {
      print_flush!("{}", msg);
    }
    Ok(())
  }

  fn println(&self, msg: &str) {
    if cfg!(debug_assertions) {
      println!("{}", msg);
    }
  }

  pub fn update(self) -> crate::Result<Status> {
    self.println(&format!(
      "Checking current version... v{}",
      self.current_version
    ));

    if self.backend.is_uptodate(self.current_version.clone())? {
      return Ok(Status::UpToDate(self.current_version.clone()));
    }

    let bin_install_path = env::current_exe()?;
    let download_url = self.backend.update_url(self.current_version.clone())?;

    if cfg!(debug_assertions) {
      println!("\n{} release status:", self.bin_name);
      println!("  * Current exe: {:?}", bin_install_path);
      println!("  * New exe download url: {:?}", download_url);
      println!(
        "\nThe new release will be downloaded/extracted and the existing binary will be replaced."
      );
    }

    let tmp_dir_parent = bin_install_path
      .parent()
      .ok_or_else(|| crate::ErrorKind::Updater("Failed to determine parent dir".into()))?;
    let tmp_dir =
      tempdir::TempDir::new_in(&tmp_dir_parent, &format!("{}_download", self.bin_name))?;

    self.println("Downloading...");
    let mut downloader = http::Download::from_url(download_url.clone());
    if let Some(ref on_progress) = self.on_progress {
      downloader.on_progress(on_progress);
    }

    let (filename, downloaded_path) = downloader.download_to(&tmp_dir.path())?;
    if is_download_installable(filename.clone()) {
      install_update(downloaded_path)?;
    } else if is_download_valid(downloaded_path.clone()) {
       self.print_flush("Extracting archive... ")?;
      let extract_path = tmp_dir.path().join("extracted");
      Extract::from_source(&downloaded_path).extract_into(&extract_path)?;
      let entries = std::fs::read_dir(extract_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
      match entries.first() {
        Some(entry) => {
          install_update(entry.to_path_buf())?;
        },
        None => {
          bail!(
            crate::ErrorKind::Updater,
            "can't read extracted dir"
          )
        }
      }
    } else {
      bail!(
        crate::ErrorKind::Updater,
        "invalid file {}",
        filename
      )
    }

    self.println("Done");
    Ok(Status::Updated(self.current_version))
  }
}

#[cfg(windows)]
fn install_update(path: PathBuf) -> crate::Result<()> {
  Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_update(path: PathBuf) -> crate::Result<()> {
  Ok(())
}

#[cfg(target_os = "macos")]
fn install_update(path: PathBuf) -> crate::Result<()> {
  Ok(())
}

fn is_download_installable(filename: String) -> bool {
  filename.ends_with(".deb") || filename.ends_with(".exe") || filename.ends_with(".app")
}

fn is_download_valid(path: PathBuf) -> bool {
  match path.extension() {
    Some(ext) => {
      let ext = ext.to_str();
      ext == Some("gz")
        || ext == Some("zip")
        || is_download_installable(path.to_string_lossy().to_string())
    }
    None => is_download_installable(path.to_string_lossy().to_string()),
  }
}
