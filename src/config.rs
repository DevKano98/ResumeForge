use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub frontend_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            data_dir: PathBuf::from("data"),
            frontend_dir: PathBuf::from("frontend"),
        }
    }
}
