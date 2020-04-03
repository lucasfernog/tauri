use regex::Regex;
use reqwest::{self, header};
use std::boxed::Box;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct Download<'a> {
  url: String,
  headers: reqwest::header::HeaderMap,
  on_progress: Option<Box<dyn Fn(f64) + 'a>>,
}
impl<'a> Download<'a> {
  /// Specify download url
  pub fn from_url(url: String) -> Self {
    Self {
      url,
      headers: reqwest::header::HeaderMap::new(),
      on_progress: None,
    }
  }

  /// Set the download request headers
  pub fn set_headers(&mut self, headers: reqwest::header::HeaderMap) -> &mut Self {
    self.headers = headers;
    self
  }

  pub fn on_progress<F: Fn(f64) + 'a>(&mut self, handler: F) -> &mut Self {
    self.on_progress = Some(Box::new(handler));
    self
  }

  /// Download the file behind the given `url` into the specified `dest`.
  /// Show a sliding progress bar if specified.
  /// If the resource doesn't specify a content-length, the progress bar will not be shown
  ///
  /// * Errors:
  ///     * `reqwest` network errors
  ///     * Unsuccessful response status
  ///     * Progress-bar errors
  ///     * Reading from response to `BufReader`-buffer
  ///     * Writing from `BufReader`-buffer to `File`
  pub fn download_to(self, dest_dir: &Path) -> crate::Result<(String, PathBuf)> {
    use io::BufRead;
    let mut headers = self.headers.clone();
    if !headers.contains_key(header::USER_AGENT) {
      headers.insert(
        header::USER_AGENT,
        "tauri/self-update".parse().expect("invalid user-agent"),
      );
    }

    set_ssl_vars!();
    let resp = reqwest::blocking::Client::new()
      .get(&self.url)
      .headers(headers)
      .send()?;
    let size = resp
      .headers()
      .get(reqwest::header::CONTENT_LENGTH)
      .map(|val| {
        val
          .to_str()
          .map(|s| s.parse::<u64>().unwrap_or(0))
          .unwrap_or(0)
      })
      .unwrap_or(0);
    if !resp.status().is_success() {
      bail!(
        crate::ErrorKind::Download,
        "Download request failed with status: {:?}",
        resp.status()
      )
    }

    let content_disposition_header = resp.headers().get(header::CONTENT_DISPOSITION);
    if let Some(content_disposition) = content_disposition_header {
      let re = Regex::new("filename=(.+)")?;
      let content_disposition_str = content_disposition
        .to_str()
        .expect("failed to convert content_disposition to string");
      let mut iter = re.captures_iter(content_disposition_str);
      match iter.next() {
        Some(filename) => {
          let filename = &filename[1].to_string();
          let mut dest_path = dest_dir.to_path_buf();
          dest_path.push(filename);
          let mut dest = fs::File::create(&dest_path)?;

          let mut src = io::BufReader::new(resp);
          let mut downloaded = 0;
          loop {
            let n = {
              let buf = src.fill_buf()?;
              dest.write_all(&buf)?;
              buf.len()
            };
            if n == 0 {
              break;
            }
            src.consume(n);
            downloaded = std::cmp::min(downloaded + n as u64, size);
            if let Some(on_progress) = &self.on_progress {
              on_progress(downloaded as f64 / size as f64);
            }

            // TODO send downloaded as progress
          }
          Ok((filename.to_string(), dest_path))
        }
        None => bail!(
          crate::ErrorKind::Download,
          "Couldn't get filename from content_disposition header: {}",
          content_disposition_str
        ),
      }
    } else {
      bail!(
        crate::ErrorKind::Download,
        "content_disposition header not found"
      )
    }
  }
}
