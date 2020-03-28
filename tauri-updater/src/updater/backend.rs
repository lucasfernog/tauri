pub trait Backend {
  fn is_uptodate(&self, version: String) -> Result<bool, String>;
  fn update_url(&self, version: String) -> Result<String, String>;
}
