use std::env;
use std::boxed::Box;

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
  on_progress: Option<Box<dyn Fn(f32)>>,
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

  pub fn on_progress<F: Fn(f32) + 'static>(mut self, handler: F) -> Self {
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
  on_progress: Option<Box<dyn Fn(f32)>>,
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
    let downloader = http::Download::from_url(download_url.clone());
    let tmp_archive_path = downloader
      .download_to(&tmp_dir.path())?;

    self.print_flush("Extracting archive... ")?;
    Extract::from_source(&tmp_archive_path)
      .extract_into(&tmp_dir.path())?;
    let new_exe = tmp_dir.path();
    self.println("Done");

    self.print_flush("Replacing binary file... ")?;
    let tmp_file = tmp_dir.path().join(&format!("__{}_backup", self.bin_name));
    Move::from_source(&new_exe)
      .replace_using_temp(&tmp_file)
      .to_dest(&bin_install_path)?;
    self.println("Done");
    Ok(Status::Updated(self.current_version))
  }
}
